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
    /// Where policy forbids starting a chat without the sandbox (#1343): why, naming the policy
    /// and who set it. The picker then offers no opt-out.
    pub locked: Option<String>,
}

impl SandboxAhead {
    /// The row for `ahead`, or none where the project has not turned the sandbox on.
    pub fn of(ahead: Ahead) -> Option<Self> {
        let row = |state: &str, said: String, install| Self {
            state: state.to_owned(),
            said,
            install,
            locked: None,
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
    let row = SandboxAhead {
        locked: sandbox::policy::Locks::of(root).opt_out_refused(),
        ..row
    };
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
    /// Whether chats here run sandboxed: the project turned the sandbox on, or an
    /// administrator's policy requires it on this machine (`policy.required`, #1423).
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
    /// How the project's Internet access presets changed since this machine last told the
    /// person (#1385): the one-time Notice each teammate sees, so a preset that widens never
    /// widens unseen. `null` when nothing did.
    pub presets_changed: Option<PresetsChanged>,
    /// Every preset a project may turn on, in the core's order, each with the hosts it lets a
    /// chat reach here (#1340): Settings › Sandbox draws them, and lists no host of its own.
    pub presets: Vec<SandboxPreset>,
    /// Each persona's own hosts, where the project has the sandbox on (#1362).
    pub persona_hosts: Vec<PersonaHosts>,
    /// What every chat here reaches and writes on this machine besides its presets (#1340), as
    /// the core grants it (`sandbox::besides`): none where the sandbox is off.
    pub besides: SandboxBesides,
    /// An administrator's policy on this machine (#1343): what it locks, and who set it. `null`
    /// where there is none.
    pub policy: Option<SandboxPolicy>,
}

/// An administrator's policy (`sandbox::policy::Locks`), as Settings shows what it locks: each
/// locked value says [`Self::locked_by`] and offers no control.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SandboxPolicy {
    /// "Locked by policy, set by <who> in <file>.": said beside every value it locks.
    pub locked_by: String,
    /// The presets a project may turn on, by word, where the policy fixes them.
    pub presets: Option<Vec<String>>,
    /// The hosts any level may add, where the policy fixes them.
    pub hosts: Option<Vec<String>>,
    /// Whether your own hosts on this machine are forbidden.
    pub personal_hosts: bool,
    /// Whether a persona's own hosts are forbidden.
    pub persona_hosts: bool,
    /// Whether starting a chat without the sandbox is forbidden.
    pub opt_out: bool,
    /// Where the policy requires the sandbox (#1423): "On, required by policy, set by <who> in
    /// <file>.", which Settings says of the mode, with no control. One that forbids the opt-out
    /// does, in every project on this machine. `null` where it does not.
    pub required: Option<String>,
    /// Whether every folder a grant would let a chat write is forbidden.
    pub write_grants: bool,
}

impl SandboxPolicy {
    /// What `locks` lock, or none where no policy is in force.
    pub fn of(locks: &sandbox::policy::Locks) -> Option<Self> {
        locks.any().then(|| Self {
            locked_by: locks.locked_by(),
            presets: locks.fixes_presets().then(|| {
                locks
                    .presets(&sandbox::Preset::ALL)
                    .into_iter()
                    .map(|preset| preset.word().to_owned())
                    .collect()
            }),
            hosts: locks
                .allowed_hosts()
                .map(|hosts| hosts.iter().map(ToString::to_string).collect()),
            personal_hosts: locks.forbids_personal_hosts(),
            persona_hosts: locks.forbids_persona_hosts(),
            opt_out: locks.forbids_opt_out(),
            required: locks.required_by(),
            write_grants: locks.forbids_write_grants(),
        })
    }
}

/// `sandbox::Besides`, counted for the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, specta::Type)]
pub struct SandboxBesides {
    /// The project's own hosts every chat here is granted.
    pub project_hosts: u32,
    /// Your own hosts every chat here is granted on this machine.
    pub your_hosts: u32,
    /// The folders you let every chat here write on this machine.
    pub folders: u32,
}

impl From<sandbox::Besides> for SandboxBesides {
    fn from(said: sandbox::Besides) -> Self {
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Self {
            project_hosts: count(said.project_hosts),
            your_hosts: count(said.your_hosts),
            folders: count(said.folders),
        }
    }
}

/// One Internet access preset (`sandbox::Preset`), as Settings shows it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SandboxPreset {
    /// The word the committed file names it by: `model-providers`.
    pub word: String,
    /// Its name in the window: `AI providers`.
    pub title: String,
    /// The hosts it lets a chat in this project reach: its own, and for code hosting the
    /// project's forges' too.
    pub hosts: Vec<String>,
    /// Whether a chat may write the project's own package caches while it is on
    /// (`sandbox::Preset::widens_caches`, #1422): the window keeps no copy of which one does.
    pub widens_caches: bool,
}

/// The hosts one persona's chats reach besides the project's (`[sandbox.personas.<name>]`),
/// and whether the person allowed them on this machine (#1362, D-1362-7).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct PersonaHosts {
    pub persona: String,
    /// Each as the project's file lists it.
    pub hosts: Vec<String>,
    /// Those a chat would reach, which an Allow allows: none an administrator's policy locks
    /// out. Empty where policy forbids persona hosts, and then nothing is asked.
    pub reached: Vec<String>,
    /// What an Allow of exactly this list sends back (`allow_persona_hosts`).
    pub digest: String,
    /// Whether the person allowed exactly this list here. Until then no chat reaches them, and
    /// the project view's Notice asks.
    pub allowed: bool,
    /// Whether it is the project's default persona, so its hosts reach every chat that names
    /// no persona too: part of what an Allow is of (D-1362-12).
    pub default: bool,
    /// Whether an Allow was kept here and the list, or its default-ness, has changed since: it
    /// grants nothing, and the Notice asks anew (D-1362-13).
    pub waiting: bool,
}

/// Every preset as the project at `plane` would have it reach under `locks`: a forge's host a
/// policy does not allow is not listed (D-1343-10).
fn presets_of(plane: &sandbox::Plane, locks: &sandbox::policy::Locks) -> Vec<SandboxPreset> {
    sandbox::Preset::ALL
        .into_iter()
        .map(|preset| SandboxPreset {
            word: preset.word().to_owned(),
            title: preset.title().to_owned(),
            hosts: sandbox::hosts(&[preset], plane, locks),
            widens_caches: preset.widens_caches(),
        })
        .collect()
}

/// Each persona's own hosts in the project at `root`, read as `plane`, under `locks`, and
/// whether the person allowed them here ([`sandbox::persona::shown_in`]).
fn persona_hosts_of(
    root: &std::path::Path,
    plane: &sandbox::Plane,
    locks: &sandbox::policy::Locks,
) -> Vec<PersonaHosts> {
    sandbox::persona::shown_in(root, plane, locks)
        .into_iter()
        .map(|one| PersonaHosts {
            allowed: one.allowed(),
            persona: one.persona,
            hosts: one.listed.iter().map(ToString::to_string).collect(),
            reached: one.hosts.iter().map(ToString::to_string).collect(),
            default: one.default,
            waiting: one.standing == sandbox::persona::Standing::Waiting,
            digest: one.digest,
        })
        .collect()
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

/// The project's presets as they changed (`sandbox::local::PresetsChange`): each named as the
/// window names it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct PresetsChanged {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// One sentence for each preset turned on that widens what a chat may do past its hosts.
    pub widens: Vec<String>,
    /// The whole set now, by the committed file's words: what the Notice sends back once read.
    pub now: Vec<String>,
}

fn state_of(root: &std::path::Path) -> SandboxState {
    state_on(root, sandbox::Os::this())
}

/// [`state_of`] on `os`.
fn state_on(root: &std::path::Path, os: sandbox::Os) -> SandboxState {
    let plane = sandbox::Plane::read(root);
    let locks = sandbox::policy::Locks::of(root);
    // The project's own, or the one an administrator's policy requires (D-1423-1).
    let on = plane.in_force(&locks).is_some();
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
        presets_changed: sandbox::local::presets_changed(root).map(|change| PresetsChanged {
            added: change.added,
            removed: change.removed,
            widens: change.widens,
            now: change.now,
        }),
        presets: presets_of(&plane, &locks),
        persona_hosts: persona_hosts_of(root, &plane, &locks),
        besides: sandbox::besides(root, &plane, &sandbox::Machine::this()).into(),
        policy: SandboxPolicy::of(&locks),
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

/// A handed-off chat that holds the asking chat's persona grants instead of its own (#1362,
/// D-1362-5), as its tab's Notice says it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct GrantsHeld {
    /// The chat that opened it by a handoff, by the name the person saw; `null` for a chat a
    /// Resume of a session record started.
    pub from: Option<String>,
    /// The persona it was opened as, whose hosts it cannot reach yet.
    pub persona: Option<String>,
    /// Where an administrator's policy forbids a persona's own hosts (#1343): why, naming the
    /// policy and who set it. Allowing them would reach nothing, so the Notice offers no Allow.
    pub locked: Option<String>,
    /// Whether the persona's hosts also wait for the person's Allow on this machine (D-1362-7):
    /// lifting the hold alone then reaches none of them.
    pub waits_here: bool,
}

/// Whether `persona`'s hosts in the project at `root` wait for the person's Allow on this
/// machine: it lists hosts a chat would reach, and they are not allowed as they stand.
pub(crate) fn persona_hosts_wait(root: &std::path::Path, persona: Option<&str>) -> bool {
    let Some(persona) = persona else {
        return false;
    };
    sandbox::persona::shown(root)
        .into_iter()
        .any(|one| one.persona == persona && !one.hosts.is_empty() && !one.allowed())
}

/// Why a persona's own hosts reach no chat in the project at `root`, where policy says so.
pub(crate) fn persona_hosts_locked(root: &std::path::Path) -> Option<String> {
    let locks = sandbox::policy::Locks::of(root);
    locks.forbids_persona_hosts().then(|| {
        format!(
            "Policy forbids a persona's own hosts. {}",
            locks.locked_by()
        )
    })
}

/// Whether chat `session` holds the asking chat's persona grants instead of its own, and whose
/// (#1362): `null` for a chat that holds its own.
#[tauri::command]
#[specta::specta]
pub fn persona_grants_held(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<Option<GrantsHeld>, String> {
    let held = planes.held(&plane)?;
    let locked = persona_hosts_locked(held.root());
    Ok(held
        .chats()
        .grants_held(session)
        .map(|(from, persona)| GrantsHeld {
            waits_here: persona_hosts_wait(held.root(), persona.as_deref()),
            from,
            persona,
            locked,
        }))
}

/// The person allowed chat `session` its own persona's grants from its tab's Notice (#1362):
/// they apply from its next start, since a chat's sandbox is compiled as it starts.
#[tauri::command]
#[specta::specta]
pub fn allow_persona_grants(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<bool, String> {
    let held = planes.held(&plane)?;
    Ok(held.chats().allow_own_grants(session))
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

/// The person read the Notice of the project's Internet access presets as it showed them,
/// `shown` (#1385): it is not shown again until they change from that.
#[tauri::command]
#[specta::specta]
pub fn acknowledge_project_presets(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    shown: Vec<String>,
) -> Result<SandboxState, String> {
    let held = planes.held(&plane)?;
    acknowledge_presets(held.root(), &shown)
}

fn acknowledge_presets(root: &std::path::Path, shown: &[String]) -> Result<SandboxState, String> {
    sandbox::local::acknowledge_presets(root, shown).map_err(|err| err.to_string())?;
    Ok(state_of(root))
}

fn acknowledge(root: &std::path::Path, shown: &[String]) -> Result<SandboxState, String> {
    sandbox::local::acknowledge_hosts(root, shown).map_err(|err| err.to_string())?;
    Ok(state_of(root))
}

/// **Allow** on a persona's hosts' Notice (#1362, D-1362-7): the person lets chats running as
/// `persona` reach its committed hosts on this machine, exactly as the Notice showed them,
/// `digest`. A list that changed since it was shown is refused, and nothing is kept. The window's
/// alone (`WINDOW_ONLY`): no link and no chat makes it.
#[tauri::command]
#[specta::specta]
pub fn allow_persona_hosts(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    persona: String,
    digest: String,
) -> Result<SandboxState, String> {
    let held = planes.held(&plane)?;
    let root = held.root().to_path_buf();
    allow_persona(
        &root,
        &persona,
        &digest,
        &|number, audited| held.hooks().record_grant(&root, number, audited),
        now_secs(),
    )?;
    Ok(state_of(&root))
}

/// [`allow_persona_hosts`] in the project at `root`: checked against the list as it stands,
/// audited, then kept with the digest of what was shown, and recorded for the Granted list.
/// Chats already running take it at their next start.
fn allow_persona(
    root: &std::path::Path,
    persona: &str,
    digest: &str,
    audit: Audit<'_>,
    at: u64,
) -> Result<(), String> {
    let shown = sandbox::persona::shown_now(root, persona, digest)?;
    let hosts: Vec<String> = shown.hosts.iter().map(ToString::to_string).collect();
    audit(
        None,
        &sandbox::grant::Audited {
            granted: true,
            what: sandbox::local::PERSONA_HOSTS,
            target: &format!(
                "{persona}: {}{}",
                hosts.join(", "),
                if shown.default {
                    " (the default persona: every chat that names no persona too)"
                } else {
                    ""
                }
            ),
            level: sandbox::grant::Level::You,
        },
    )?;
    sandbox::local::allow_persona_hosts(root, persona, &shown.digest)
        .map_err(|why| format!("purlis could not keep it, so nothing was allowed: {why}"))?;
    if let Err(why) = sandbox::local::record_made(
        root,
        sandbox::local::Made {
            what: sandbox::local::PERSONA_HOSTS.to_owned(),
            target: persona.to_owned(),
            level: sandbox::grant::Level::You.word().to_owned(),
            at,
            chat: None,
        },
    ) {
        // The Allow stands, and is audited; only the Granted list's "when" is lost.
        tracing::warn!("purlis: a persona's hosts were allowed without when ({why})");
    }
    Ok(())
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

// ---- a block's Allow, and every grant (#1342, #1348) -----------------------------------------

/// What a grant names, as the window sends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum GrantWhat {
    /// A host to reach.
    Host,
    /// A folder to write, and everything in it.
    Write,
    /// A vault a persona's chats may use although the registry does not tag it for the
    /// persona (#1430). Granted from a refused vault's Notice (`crate::vaultroute`), never from
    /// a block's Allow.
    Vault,
    /// A persona's committed hosts, which you allowed on this machine (#1362). Allowed from
    /// their own Notice (`allow_persona_hosts`), never from a block's Allow.
    PersonaHosts,
}

impl GrantWhat {
    fn word(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Write => "write",
            Self::Vault => sandbox::local::VAULT,
            Self::PersonaHosts => sandbox::local::PERSONA_HOSTS,
        }
    }

    fn of_word(word: &str) -> Option<Self> {
        [Self::Host, Self::Write, Self::Vault, Self::PersonaHosts]
            .into_iter()
            .find(|what| what.word() == word)
    }
}

/// Who a grant is for, as the window sends it ([`sandbox::grant::Level`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum GrantLevel {
    /// This chat, while the app holds it.
    Chat,
    /// Every chat of this project on this machine.
    You,
    /// Everyone in the project: committed. A host only.
    Project,
}

impl From<GrantLevel> for sandbox::grant::Level {
    fn from(level: GrantLevel) -> Self {
        match level {
            GrantLevel::Chat => Self::Chat,
            GrantLevel::You => Self::You,
            GrantLevel::Project => Self::Project,
        }
    }
}

impl From<sandbox::grant::Level> for GrantLevel {
    fn from(level: sandbox::grant::Level) -> Self {
        match level {
            sandbox::grant::Level::Chat => Self::Chat,
            sandbox::grant::Level::You => Self::You,
            sandbox::grant::Level::Project => Self::Project,
        }
    }
}

/// What allowing a block answered (#1342): the sentence the Notice says. The chat is then owed
/// a restart on its conversation, which the window asks for once its turn has ended.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Allowed {
    pub said: String,
}

/// Seconds since 1970, now.
pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Writes the audit of a grant or revoke, and answers whether it was written.
pub(crate) type Audit<'a> =
    &'a dyn Fn(Option<u32>, &sandbox::grant::Audited<'_>) -> Result<(), String>;

/// The project's file a host is kept in at `level`: yours or the committed one.
fn hosts_file(level: sandbox::grant::Level) -> purlis_core::settings::Which {
    if level == sandbox::grant::Level::Project {
        purlis_core::settings::Which::Shared
    } else {
        purlis_core::settings::Which::Local
    }
}

/// **Allows what chat `session`'s sandbox blocked** (#1342), at `level`, in the project at
/// `root` on `machine`: the core judges `target` again (the window's word and the chat's count
/// for nothing), the audit is written, then the grant is kept where its level keeps it and the
/// chat is owed a restart on its conversation. Never a class 1–3 path, never a folder off the
/// allowlist (D-1342-10), never a folder for the project, never past a policy lock.
fn allow(
    root: &std::path::Path,
    machine: &sandbox::Machine,
    chats: &crate::chats::Chats,
    session: u32,
    asked: (GrantWhat, &str, GrantLevel),
    audit: Audit<'_>,
    at: u64,
) -> Result<Allowed, String> {
    let folder = chats.folder_of(session);
    let (what, level) = judged(root, machine, folder.as_deref(), session, asked)?;
    kept(root, chats, session, (&what, level), audit, at)?;
    Ok(Allowed {
        said: format!(
            // No target in the sentence: it is the chat's choice, and the window shows it apart.
            "Allowed {}. The chat restarts on the same conversation once its turn ends, and is \
             told to retry.",
            level.said()
        ),
    })
}

/// **What allowing `target` for chat `session` would grant**, judged by the core and never
/// taken from the window's word: a host by the project's own hosts' rules, a folder against
/// the allowlist from `folder`, the folder the chat was started in (none for a chat that is not
/// open), and either past an administrator's policy (#1343). Nothing is kept or audited here.
pub(crate) fn judged(
    root: &std::path::Path,
    machine: &sandbox::Machine,
    folder: Option<&std::path::Path>,
    session: u32,
    (what, target, level): (GrantWhat, &str, GrantLevel),
) -> Result<(sandbox::grant::What, sandbox::grant::Level), String> {
    use sandbox::grant::{self, Level, What};
    let level = Level::from(level);
    if level == Level::Project && what == GrantWhat::Write {
        return Err(
            "purlis will not allow a folder for everyone in the project: a folder is a path on \
             this machine. Allow it for this chat, or for every chat of this project on this \
             machine."
                .to_owned(),
        );
    }
    let what = match what {
        GrantWhat::Vault => {
            return Err(
                "purlis allows a vault from the notice its refusal raises, not from a block's."
                    .to_owned(),
            );
        }
        GrantWhat::PersonaHosts => {
            return Err(
                "purlis allows a persona's hosts from the notice that shows them, not from a \
                 block's."
                    .to_owned(),
            );
        }
        GrantWhat::Host => What::Host(grant::host(target).map_err(|why| why.to_string())?),
        GrantWhat::Write => {
            let folder = folder.ok_or_else(|| {
                format!("purlis did not allow anything for chat {session}: it is not open.")
            })?;
            let ground = grant::Ground::of(root, folder, machine);
            What::Write(grant::write(target, &ground.place()).map_err(|why| why.to_string())?)
        }
    };
    // Policy (#1343): the strictest wins, for a host at the level it would be kept at, and for
    // any folder at all.
    if let Some(why) = sandbox::policy::Locks::of(root).refuses_grant(&what, level) {
        return Err(why);
    }
    Ok((what, level))
}

/// **Keeps a grant [`judged`] made for chat `session`**: audited first, then kept where
/// `level` keeps it, and the chat owed a restart on its conversation. An audit that cannot be
/// written keeps nothing.
pub(crate) fn kept(
    root: &std::path::Path,
    chats: &crate::chats::Chats,
    session: u32,
    (what, level): (&sandbox::grant::What, sandbox::grant::Level),
    audit: Audit<'_>,
    at: u64,
) -> Result<(), String> {
    use sandbox::grant::{self, Level, What};
    let target = what.target();
    audit(
        Some(session),
        &grant::Audited {
            granted: true,
            what: what.word(),
            target: &target,
            level,
        },
    )?;
    let told = grant::told(what, level);
    match (what, level) {
        (_, Level::Chat) => chats.grant(session, what.clone(), at, told)?,
        (What::Host(host), Level::You | Level::Project) => {
            purlis_core::settings::hosts::grant(root, hosts_file(level), host)?;
            chats.owe_restart(session, told);
        }
        (What::Write(folder), _) => {
            sandbox::local::grant_write(root, folder)
                .map_err(|why| format!("purlis could not keep {}: {why}", folder.display()))?;
            chats.owe_restart(session, told);
        }
    }
    if level != Level::Chat
        && let Err(why) = sandbox::local::record_made(
            root,
            sandbox::local::Made {
                what: what.word().to_owned(),
                target,
                level: level.word().to_owned(),
                at,
                chat: None,
            },
        )
    {
        // The grant stands, and is audited; only the Granted list's "when" is lost.
        tracing::warn!("purlis: a sandbox grant was kept without when it was made ({why})");
    }
    Ok(())
}

/// **Allow** on a block's Notice (#1342): `shown` is the block and the host or folder the
/// Notice showed whole (or the host the person typed, where the block named none). Refused
/// whole unless the chat is held on that block now (#1538, [`crate::taskblocks::shown_one`]). The window then restarts the chat once its turn has ended
/// (`restart_chat`).
#[tauri::command]
#[specta::specta]
pub fn allow_sandbox_block(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    shown: crate::taskblocks::BlockShown,
    level: GrantLevel,
) -> Result<Allowed, String> {
    let held = planes.held(&plane)?;
    let block = crate::taskblocks::shown_one(held.chats().blocks(), session, &shown)?;
    let (what, target) = (shown.what, shown.target);
    let root = held.root().to_path_buf();
    let allowed = allow(
        &root,
        &sandbox::Machine::this(),
        held.chats(),
        session,
        (what, &target, level),
        &|number, audited| held.hooks().record_grant(&root, number, audited),
        now_secs(),
    )?;
    // Answered: neither this Notice nor a question for several tasks answers it again.
    held.chats().blocks().answered(session, &block);
    Ok(allowed)
}

/// The chats of this project owed a restart (#1342, #1428): to take a grant, or because the
/// person asked. For the window that drives it once each one's turn has ended.
#[tauri::command]
#[specta::specta]
pub fn owed_restarts(planes: tauri::State<'_, Planes>, plane: PlaneId) -> Result<Vec<u32>, String> {
    Ok(planes.held(&plane)?.chats().owed_restarts())
}

/// One grant, as Settings' Granted list shows it (#1348).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SandboxGrant {
    /// What Revoke is sent by.
    pub id: String,
    pub what: GrantWhat,
    /// The host, the folder, the vault's name, or a persona's hosts.
    pub target: String,
    /// For a vault, the persona whose chats may use it; for a persona's hosts, that persona.
    pub persona: Option<String>,
    pub level: GrantLevel,
    /// Who committed it, for one the project carries; null for one you granted, or one the
    /// project's file holds that is not committed yet.
    pub by: Option<String>,
    /// When, in seconds since 1970, where that is known.
    pub at: Option<u32>,
    /// The chat it was granted from, where it came from one.
    pub chat: Option<String>,
    /// Why an administrator's policy locks it out, where one does: it is kept, and reaches or
    /// writes nothing while the policy stands. The list marks it "Locked by policy".
    pub locked: Option<String>,
    /// For a persona's hosts you allowed whose list has changed since: why it grants nothing.
    /// It stays listed, with Revoke, and never returns to force unless allowed anew.
    pub waiting: Option<String>,
    /// For a persona's hosts: the persona is the project's default, so they reach every chat
    /// that names no persona too.
    pub for_no_persona: bool,
}

/// Who last committed a line naming `host` in the project's committed file, and when: the
/// project's history, asked of git. `None` where git has nothing to say (not committed yet).
fn committed_by(root: &std::path::Path, host: &str) -> Option<(String, u64)> {
    let manifest = purlis_core::names::manifest(root);
    let file = manifest.file_name()?.to_str()?.to_owned();
    // Through the hardened runner: no hook, no inherited `GIT_*`, and the project's `.git`
    // read by purlis where it is a file (#1055).
    let quoted = format!("\"{host}\"");
    let run = purlis_core::worktree::git::run(
        root,
        &[
            "log",
            "-1",
            "--format=%an%x09%at",
            "-S",
            &quoted,
            "--",
            &file,
        ],
        purlis_core::worktree::git::READ,
    )
    .ok()?;
    let line = run.out;
    let (name, at) = line.trim().split_once('\t')?;
    Some((name.to_owned(), at.parse().ok()?))
}

/// The separator of a grant's id: no host or folder purlis grants holds it.
const SEP: char = '\u{1f}';

/// **Every grant in force for the project at `root`** (#1348): each open chat's (a closed
/// chat's ended with it, D-1348-1), yours (hosts and folders), and the project's own hosts.
fn grants_of(root: &std::path::Path, chats: &crate::chats::Chats) -> Vec<SandboxGrant> {
    use sandbox::grant::Level;
    let made = sandbox::local::made(root);
    let when = |what: GrantWhat, target: &str, level: Level| {
        made.iter()
            .find(|one| {
                one.what == what.word() && one.target == target && one.level == level.word()
            })
            .and_then(|one| u32::try_from(one.at).ok())
    };
    let row = |what: GrantWhat, target: String, level: Level, at| SandboxGrant {
        id: format!("{}{SEP}{}{SEP}{target}", level.word(), what.word()),
        what,
        target,
        persona: None,
        level: level.into(),
        by: None,
        at,
        chat: None,
        locked: None,
        waiting: None,
        for_no_persona: false,
    };
    let mut out: Vec<SandboxGrant> = chats
        .chat_grants()
        .into_iter()
        .filter_map(|(id, one)| {
            let what = GrantWhat::of_word(one.what.word())?;
            let target = one.what.target();
            Some(SandboxGrant {
                id: format!("chat{SEP}{id}{SEP}{}{SEP}{target}", what.word()),
                chat: Some(one.chat),
                ..row(what, target, Level::Chat, u32::try_from(one.at).ok())
            })
        })
        .collect();
    for host in sandbox::hosts::personal(root) {
        let target = host.to_string();
        let at = when(GrantWhat::Host, &target, Level::You);
        out.push(row(GrantWhat::Host, target, Level::You, at));
    }
    for folder in sandbox::local::granted_writes(root) {
        let target = folder.display().to_string();
        let at = when(GrantWhat::Write, &target, Level::You);
        out.push(row(GrantWhat::Write, target, Level::You, at));
    }
    let locks = sandbox::policy::Locks::of(root);
    if let Some(policy) = sandbox::Plane::read(root).in_force(&locks) {
        for host in policy.hosts {
            let target = host.to_string();
            let committed = committed_by(root, &target);
            let at = committed
                .as_ref()
                .and_then(|(_, at)| u32::try_from(*at).ok())
                .or_else(|| when(GrantWhat::Host, &target, Level::Project));
            out.push(SandboxGrant {
                by: committed.map(|(name, _)| name),
                ..row(GrantWhat::Host, target, Level::Project, at)
            });
        }
    }
    // The vaults you let a persona's chats use here (#1430): locked out, not gone, where policy
    // forbids them, so the list says why they no longer open.
    let vaults_locked = locks.vault_grants_refused();
    for grant in sandbox::local::granted_vaults(root) {
        let recorded = made
            .iter()
            .find(|one| one.what == sandbox::local::VAULT && one.target == grant.target());
        out.push(SandboxGrant {
            id: format!(
                "you{SEP}{}{SEP}{}{SEP}{}",
                sandbox::local::VAULT,
                grant.vault,
                grant.persona
            ),
            persona: Some(grant.persona),
            at: recorded.and_then(|one| u32::try_from(one.at).ok()),
            chat: recorded.and_then(|one| one.chat.clone()),
            locked: vaults_locked.clone(),
            ..row(GrantWhat::Vault, grant.vault, Level::You, None)
        });
    }
    // Each persona's hosts you allowed here (#1362), credited to the persona: one row for the
    // list, since an Allow is of the list as it was shown. One whose list changed since grants
    // nothing and says so, with Revoke (D-1362-13).
    let shown = sandbox::persona::shown_in(root, &sandbox::Plane::read(root), &locks);
    for kept in sandbox::local::allowed_persona_hosts(root) {
        let now = shown.iter().find(|one| one.persona == kept.persona);
        let in_force = now.is_some_and(sandbox::persona::Shown::allowed);
        let hosts: Vec<String> = now
            .map(|one| one.hosts.iter().map(ToString::to_string).collect())
            .unwrap_or_default();
        out.push(SandboxGrant {
            id: format!(
                "you{SEP}{}{SEP}{}",
                sandbox::local::PERSONA_HOSTS,
                kept.persona
            ),
            at: when(GrantWhat::PersonaHosts, &kept.persona, Level::You),
            waiting: (!in_force).then(|| {
                "waiting: the list changed since you allowed it, so it reaches nothing until \
                 you allow it again"
                    .to_owned()
            }),
            for_no_persona: in_force && now.is_some_and(|one| one.default),
            persona: Some(kept.persona),
            ..row(
                GrantWhat::PersonaHosts,
                if hosts.is_empty() {
                    "no host now".to_owned()
                } else {
                    hosts.join(", ")
                },
                Level::You,
                None,
            )
        });
    }
    // What is kept and policy now drops is said, never listed as granted (#1423): a host at
    // the level it is kept at, and every folder where policy forbids write grants.
    for one in &mut out {
        one.locked = match one.what {
            GrantWhat::Host => sandbox::hosts::Host::parse(&one.target)
                .ok()
                .and_then(|host| {
                    locks.refuses(&sandbox::hosts::Granted {
                        host,
                        level: sandbox::grant::Level::from(one.level).hosts_level(),
                    })
                }),
            GrantWhat::Write => locks.write_grants_refused(),
            // A vault's row was given its own lock as it was listed (#1430).
            GrantWhat::Vault => one.locked.take(),
            // Only hosts a chat would reach are listed for a persona (#1362).
            GrantWhat::PersonaHosts => None,
        };
    }
    out
}

/// **Revokes the grant called `id`** ([`grants_of`]) in the project at `root` (#1348): checked
/// to be there, audited, then taken out of every later start. A project host's revoke is a
/// change to the committed file, which teammates follow like any other.
fn revoke(
    root: &std::path::Path,
    chats: &crate::chats::Chats,
    id: &str,
    audit: Audit<'_>,
) -> Result<(), String> {
    use sandbox::grant::{Audited, Level, What};
    let gone = || "purlis did not revoke it: that grant is no longer there.".to_owned();
    let parts: Vec<&str> = id.split(SEP).collect();
    if let ["you", sandbox::local::VAULT, vault, persona] = parts.as_slice() {
        return revoke_vault(root, vault, persona, audit);
    }
    if let ["you", sandbox::local::PERSONA_HOSTS, persona] = parts.as_slice() {
        return revoke_persona(root, persona, audit);
    }
    let (level, chat, what, target) = match parts.as_slice() {
        ["chat", chat, what, target] => (Level::Chat, Some(*chat), *what, *target),
        [level, what, target] => (
            Level::of_word(level).ok_or_else(gone)?,
            None,
            *what,
            *target,
        ),
        _ => return Err(gone()),
    };
    let grant = match GrantWhat::of_word(what).ok_or_else(gone)? {
        // A vault's grant is named with its persona, and a persona's hosts by it, above.
        GrantWhat::Vault | GrantWhat::PersonaHosts => return Err(gone()),
        GrantWhat::Host => What::Host(sandbox::hosts::Host::parse(target).map_err(|_| gone())?),
        GrantWhat::Write => What::Write(std::path::PathBuf::from(target)),
    };
    let there = match (&grant, level, chat) {
        (_, Level::Chat, Some(chat)) => chats.holds(chat, &grant),
        (What::Host(host), Level::You, _) => sandbox::hosts::personal(root).contains(host),
        (What::Host(host), Level::Project, _) => sandbox::Plane::read(root)
            .in_force(&sandbox::policy::Locks::of(root))
            .is_some_and(|policy| policy.hosts.contains(host)),
        (What::Write(folder), Level::You, _) => {
            sandbox::local::granted_writes(root).contains(folder)
        }
        _ => false,
    };
    if !there {
        return Err(gone());
    }
    audit(
        None,
        &Audited {
            granted: false,
            what,
            target,
            level,
        },
    )?;
    match (&grant, chat) {
        (_, Some(chat)) => {
            chats.revoke(chat, &grant);
        }
        (What::Host(host), None) => {
            purlis_core::settings::hosts::revoke(root, hosts_file(level), host)?;
        }
        (What::Write(folder), None) => {
            sandbox::local::revoke_write(root, folder)
                .map_err(|why| format!("purlis could not revoke {}: {why}", folder.display()))?;
        }
    }
    if let Err(why) = sandbox::local::forget_made(root, what, target, level.word()) {
        tracing::warn!("purlis: a revoked grant's record was left behind ({why})");
    }
    Ok(())
}

/// **Revokes `persona`'s use of `vault`** in the project at `root` (#1430): checked to be
/// there, audited, then taken off this machine's record. The next brokered run reads it, so a
/// chat of that persona is refused the vault again with nothing restarted.
fn revoke_vault(
    root: &std::path::Path,
    vault: &str,
    persona: &str,
    audit: Audit<'_>,
) -> Result<(), String> {
    let grant = sandbox::local::VaultGrant {
        vault: vault.to_owned(),
        persona: persona.to_owned(),
    };
    if !sandbox::local::granted_vaults(root).contains(&grant) {
        return Err("purlis did not revoke it: that grant is no longer there.".to_owned());
    }
    audit(
        None,
        &sandbox::grant::Audited {
            granted: false,
            what: sandbox::local::VAULT,
            target: &grant.target(),
            level: sandbox::grant::Level::You,
        },
    )?;
    sandbox::local::revoke_vault(root, vault, persona)
        .map_err(|why| format!("purlis could not revoke vault {vault} for {persona}: {why}"))
}

/// **Revokes your Allow of `persona`'s hosts** in the project at `root` (#1362): checked to be
/// there, audited, then taken off this machine's record. Chats as `persona` reach them no more
/// from their next start, and the Notice asks again.
fn revoke_persona(root: &std::path::Path, persona: &str, audit: Audit<'_>) -> Result<(), String> {
    let gone = || "purlis did not revoke it: that grant is no longer there.".to_owned();
    if !sandbox::local::allowed_persona_hosts(root)
        .iter()
        .any(|kept| kept.persona == persona)
    {
        return Err(gone());
    }
    audit(
        None,
        &sandbox::grant::Audited {
            granted: false,
            what: sandbox::local::PERSONA_HOSTS,
            target: persona,
            level: sandbox::grant::Level::You,
        },
    )?;
    sandbox::local::revoke_persona_hosts(root, persona)
        .map_err(|why| format!("purlis could not revoke {persona}'s hosts: {why}"))
        .map(|_| ())
}

/// Every grant in force here, for Settings' Granted list (#1348).
#[tauri::command]
#[specta::specta]
pub fn sandbox_grants(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<Vec<SandboxGrant>, String> {
    let held = planes.held(&plane)?;
    Ok(grants_of(held.root(), held.chats()))
}

/// **Revoke** on Settings' Granted list (#1348): the grant called `id` is taken out of every
/// later start, and audited. Answers the list as it is now.
#[tauri::command]
#[specta::specta]
pub fn revoke_sandbox_grant(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    id: String,
) -> Result<Vec<SandboxGrant>, String> {
    let held = planes.held(&plane)?;
    let root = held.root().to_path_buf();
    revoke(&root, held.chats(), &id, &|number, audited| {
        held.hooks().record_grant(&root, number, audited)
    })?;
    Ok(grants_of(&root, held.chats()))
}

/// The folders you listed as ones chats in this project may be granted (D-1342-10), each as it
/// was resolved; and each one dropped from the list now because it no longer resolves to itself
/// (it, or a folder above it, was swapped for a link since: R8), for Settings to say.
#[tauri::command]
#[specta::specta]
pub fn grantable_folders(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<GrantableFolders, String> {
    let held = planes.held(&plane)?;
    Ok(listed_and_pruned(held.root()))
}

/// The folders chats may be granted, and those just dropped from the list.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct GrantableFolders {
    pub folders: Vec<String>,
    pub dropped: Vec<String>,
}

fn listed_and_pruned(root: &std::path::Path) -> GrantableFolders {
    let mut dropped = Vec::new();
    for listed in sandbox::local::grantable_folders(root) {
        if !sandbox::grant::still_itself(&listed) {
            match sandbox::local::unlist_grantable(root, &listed) {
                Ok(()) => dropped.push(listed.display().to_string()),
                Err(why) => tracing::warn!(
                    "purlis: a listed folder that resolves elsewhere was left listed ({why})"
                ),
            }
        }
    }
    GrantableFolders {
        folders: listed_folders(root),
        dropped,
    }
}

fn listed_folders(root: &std::path::Path) -> Vec<String> {
    sandbox::local::grantable_folders(root)
        .iter()
        .map(|folder| folder.display().to_string())
        .collect()
}

/// Lists `folder` as one chats here may be granted (D-1342-10), this machine only: every refusal
/// a write grant makes, but the allowlist it adds to. Answers the list as it is now, and what
/// the folder holds that later code is loaded from, for the warning Settings shows.
#[tauri::command]
#[specta::specta]
pub fn list_grantable_folder(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    folder: String,
) -> Result<GrantableListed, String> {
    let held = planes.held(&plane)?;
    list_folder(held.root(), &sandbox::Machine::this(), &folder)
}

fn list_folder(
    root: &std::path::Path,
    machine: &sandbox::Machine,
    folder: &str,
) -> Result<GrantableListed, String> {
    // Policy (#1343): a folder listed here is one a chat may be granted, which it forbids.
    if let Some(why) = sandbox::policy::Locks::of(root).write_grants_refused() {
        return Err(why);
    }
    let ground = sandbox::grant::Ground::of(root, root, machine);
    let folder =
        sandbox::grant::grantable(folder, &ground.place()).map_err(|why| why.to_string())?;
    let holds = sandbox::grant::holds_later_code(&folder, &ground.place());
    sandbox::local::list_grantable(root, &folder)
        .map_err(|why| format!("purlis could not keep {}: {why}", folder.display()))?;
    Ok(GrantableListed {
        folders: listed_folders(root),
        holds,
    })
}

/// The folders chats may be granted once one was listed, and what the one listed holds that
/// later code is loaded from, outside any sandbox: what Settings warns of.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct GrantableListed {
    pub folders: Vec<String>,
    pub holds: Vec<String>,
}

/// Takes `folder` off the folders chats here may be granted. Answers the list as it is now.
#[tauri::command]
#[specta::specta]
pub fn unlist_grantable_folder(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    folder: String,
) -> Result<Vec<String>, String> {
    let held = planes.held(&plane)?;
    sandbox::local::unlist_grantable(held.root(), std::path::Path::new(&folder))
        .map_err(|why| format!("purlis could not take {folder} off the list: {why}"))?;
    Ok(listed_folders(held.root()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- an administrator's policy (#1343), through the test build's seam ----

    #[test]
    fn a_persona_s_hosts_locked_by_policy_say_who_locked_them_and_no_policy_reads_none() {
        use sandbox::policy::{Locks, set_for_this_test};
        let root = std::path::Path::new("/home/dev/plane");
        // A test build never reads this machine's file: no policy until a test sets one.
        assert_eq!(persona_hosts_locked(root), None);
        assert_eq!(state_on_policy(root), None);
        set_for_this_test(Locks::parse(
            r#"{"owner": "IT", "sandbox": {"persona-hosts": false}}"#,
            std::path::Path::new("/etc/purlis/policy.json"),
        ));
        let said = persona_hosts_locked(root).expect("locked");
        set_for_this_test(Locks::none());
        assert_eq!(
            said,
            "Policy forbids a persona's own hosts. Locked by policy, set by IT in \
             /etc/purlis/policy.json."
        );
    }

    /// This thread's policy until it is dropped, a panic included: no later test on the thread
    /// is left under it.
    struct Under;

    impl Under {
        fn policy(json: &str) -> Self {
            sandbox::policy::set_for_this_test(sandbox::policy::Locks::parse(
                json,
                std::path::Path::new("/etc/purlis/policy.json"),
            ));
            Self
        }
    }

    impl Drop for Under {
        fn drop(&mut self) {
            sandbox::policy::set_for_this_test(sandbox::policy::Locks::none());
        }
    }

    /// D-1423-1: a policy that forbids the opt-out requires the sandbox, so a project that has
    /// not turned it on reads as on, is offered nothing, and says who requires it.
    #[test]
    fn a_project_with_no_sandbox_reads_as_on_and_required_where_policy_forbids_the_opt_out() {
        let project = tempfile::tempdir().expect("a project");
        std::fs::write(project.path().join("charter.toml"), "schema = 1\n").expect("toml");
        let free = state_of(project.path());
        assert!(!free.on && free.offer, "{free:?}");
        {
            let _under = Under::policy(r#"{"owner": "IT", "sandbox": {"opt-out": false}}"#);
            let state = state_of(project.path());
            assert!(state.on, "{state:?}");
            assert!(!state.offer, "what is on already is not offered");
            assert_eq!(
                state.policy.expect("a policy").required.as_deref(),
                Some("On, required by policy, set by IT in /etc/purlis/policy.json.")
            );
        }
        // A policy that locks something else requires nothing.
        let _under = Under::policy(r#"{"owner": "IT", "sandbox": {"write-grants": false}}"#);
        let state = state_of(project.path());
        assert!(!state.on, "{state:?}");
        assert_eq!(state.policy.expect("a policy").required, None);
    }

    /// #1423: what is kept on this machine and policy now drops is marked, never listed as
    /// granted: a folder under a write-grants lock.
    #[test]
    fn a_kept_grant_policy_now_drops_is_marked_locked_on_the_granted_list() {
        let project = tempfile::tempdir().expect("a project");
        let folder = project.path().join("out");
        sandbox::local::grant_write(project.path(), &folder).expect("kept");
        let chats = crate::chats::Chats::new();
        let free = grants_of(project.path(), &chats);
        assert_eq!(free.len(), 1, "{free:?}");
        assert_eq!(free[0].locked, None);
        let _under = Under::policy(r#"{"owner": "IT", "sandbox": {"write-grants": false}}"#);
        let held = grants_of(project.path(), &chats);
        assert_eq!(held[0].what, GrantWhat::Write);
        assert_eq!(
            held[0].locked.as_deref(),
            Some(
                "Policy forbids allowing a chat to write a folder. Locked by policy, set by IT \
                 in /etc/purlis/policy.json."
            )
        );
    }

    /// What Settings is told of the policy for the project at `root`.
    fn state_on_policy(root: &std::path::Path) -> Option<SandboxPolicy> {
        SandboxPolicy::of(&sandbox::policy::Locks::of(root))
    }

    // ---- a block's Allow, and the Granted list (#1342, #1348) ----

    fn no_audit() -> impl Fn(Option<u32>, &sandbox::grant::Audited<'_>) -> Result<(), String> {
        |_, _| Err("the event log is not open".to_owned())
    }

    fn machine() -> sandbox::Machine {
        sandbox::Machine {
            env: purlis_core::secrets::Env::of(&[]),
            home: Some(std::path::PathBuf::from("/nohome/dev")),
            os: sandbox::Os::MacOs,
        }
    }

    #[test]
    fn nothing_is_allowed_that_was_not_audited_or_that_a_grant_cannot_name() {
        let project = tempfile::tempdir().expect("a project");
        let chats = crate::chats::Chats::new();
        let refused = |what, target: &str, level| {
            allow(
                project.path(),
                &machine(),
                &chats,
                3,
                (what, target, level),
                &no_audit(),
                100,
            )
            .expect_err("refused")
        };
        assert!(
            refused(GrantWhat::Write, "/tmp/x", GrantLevel::Project)
                .contains("everyone in the project")
        );
        assert!(
            refused(GrantWhat::Host, "169.254.169.254", GrantLevel::You).contains("link-local")
        );
        // A write is judged against the chat's own folder, so a chat that is not open gets none.
        assert!(refused(GrantWhat::Write, "/tmp/x", GrantLevel::Chat).contains("not open"));
        // An unaudited grant is no grant: nothing is kept at any level.
        assert!(refused(GrantWhat::Host, "api.example.com", GrantLevel::You).contains("event log"));
        assert!(sandbox::hosts::personal(project.path()).is_empty());
        assert!(grants_of(project.path(), &chats).is_empty());
    }

    #[test]
    fn a_vault_you_allowed_a_persona_is_listed_and_revoke_takes_it_out_audited_once() {
        let project = tempfile::tempdir().expect("a project");
        let root = project.path();
        let chats = crate::chats::Chats::new();
        sandbox::local::grant_vault(root, "devops", "steward").expect("granted");
        sandbox::local::record_made(
            root,
            sandbox::local::Made {
                what: "vault".to_owned(),
                target: "devops for steward".to_owned(),
                level: "you".to_owned(),
                at: 42,
                chat: Some("steward 1".to_owned()),
            },
        )
        .expect("recorded");
        let listed = grants_of(root, &chats);
        assert_eq!(
            listed,
            [SandboxGrant {
                id: "you\u{1f}vault\u{1f}devops\u{1f}steward".to_owned(),
                what: GrantWhat::Vault,
                target: "devops".to_owned(),
                persona: Some("steward".to_owned()),
                level: GrantLevel::You,
                by: None,
                at: Some(42),
                chat: Some("steward 1".to_owned()),
                locked: None,
                waiting: None,
                for_no_persona: false,
            }]
        );
        // A block's Allow never grants one: only the refused vault's own Notice does.
        let from_a_block = allow(
            root,
            &machine(),
            &chats,
            3,
            (GrantWhat::Vault, "devops", GrantLevel::You),
            &no_audit(),
            100,
        )
        .expect_err("refused");
        assert!(
            from_a_block.contains("notice its refusal raises"),
            "{from_a_block}"
        );
        // Not audited, not revoked.
        assert!(revoke(root, &chats, &listed[0].id, &no_audit()).is_err());
        assert_eq!(grants_of(root, &chats).len(), 1);

        let heard = std::sync::Mutex::new(Vec::new());
        let audit = |number: Option<u32>, audited: &sandbox::grant::Audited<'_>| {
            heard.lock().unwrap().push((
                number,
                audited.kind(),
                audited.what.to_owned(),
                audited.target.to_owned(),
            ));
            Ok(())
        };
        revoke(root, &chats, &listed[0].id, &audit).expect("revoked");
        assert!(grants_of(root, &chats).is_empty());
        assert!(sandbox::local::granted_vaults(root).is_empty());
        assert!(sandbox::local::made(root).is_empty());
        let again = revoke(root, &chats, &listed[0].id, &audit).expect_err("not there");
        assert!(again.contains("no longer there"), "{again}");
        assert_eq!(
            *heard.lock().unwrap(),
            [(
                None,
                "trust.sandbox.revoke",
                "vault".to_owned(),
                "devops for steward".to_owned()
            )],
            "one audit, for the one revoke that happened"
        );
    }

    /// #1362, D-1362-7: a persona's committed hosts reach nothing here until the person allows
    /// them as shown; the Allow is audited, listed credited to the persona, and revocable.
    #[test]
    fn a_persona_s_hosts_are_allowed_as_shown_listed_for_the_persona_and_revoked() {
        let project = tempfile::tempdir().expect("a project");
        let root = project.path();
        std::fs::write(
            root.join("charter.toml"),
            "schema = 1\n\n[sandbox]\nmode = \"on\"\n\n[sandbox.personas.devops]\n\
             hosts = [\"10.0.0.5:6443\", \"*.internal.example\"]\n",
        )
        .expect("toml");
        let chats = crate::chats::Chats::new();
        let state = state_of(root);
        assert_eq!(state.persona_hosts.len(), 1);
        let shown = state.persona_hosts[0].clone();
        assert!(!shown.allowed);
        assert!(grants_of(root, &chats).is_empty());

        // A digest that is not the list's as it stands allows nothing.
        let stale = allow_persona(root, "devops", "not-what-stands", &no_audit(), 7)
            .expect_err("changed since");
        assert!(stale.contains("changed after they were shown"), "{stale}");
        // An Allow that is not audited is no Allow.
        assert!(allow_persona(root, "devops", &shown.digest, &no_audit(), 7).is_err());
        assert!(!state_of(root).persona_hosts[0].allowed);

        let heard = std::sync::Mutex::new(Vec::new());
        let audit = |number: Option<u32>, audited: &sandbox::grant::Audited<'_>| {
            heard.lock().unwrap().push((
                number,
                audited.kind(),
                audited.what.to_owned(),
                audited.target.to_owned(),
            ));
            Ok(())
        };
        allow_persona(root, "devops", &shown.digest, &audit, 42).expect("allowed");
        assert!(state_of(root).persona_hosts[0].allowed);
        let listed = grants_of(root, &chats);
        assert_eq!(
            listed,
            [SandboxGrant {
                id: "you\u{1f}persona-hosts\u{1f}devops".to_owned(),
                what: GrantWhat::PersonaHosts,
                target: "10.0.0.5:6443, *.internal.example".to_owned(),
                persona: Some("devops".to_owned()),
                level: GrantLevel::You,
                by: None,
                at: Some(42),
                chat: None,
                locked: None,
                waiting: None,
                for_no_persona: false,
            }]
        );
        // A teammate changes the list: the Allow grants nothing, and is listed as waiting.
        std::fs::write(
            root.join("charter.toml"),
            "schema = 1\n\n[sandbox]\nmode = \"on\"\n\n[sandbox.personas.devops]\n\
             hosts = [\"10.0.0.5:6443\"]\n",
        )
        .expect("toml");
        let now = state_of(root).persona_hosts[0].clone();
        assert!(!now.allowed);
        assert!(now.waiting);
        let waiting = grants_of(root, &chats);
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].id, listed[0].id);
        assert!(
            waiting[0]
                .waiting
                .as_deref()
                .is_some_and(|why| why.starts_with("waiting: the list changed")),
            "{waiting:?}"
        );
        // Put back as it was allowed: it stays waiting until it is allowed anew (D-1362-13).
        std::fs::write(
            root.join("charter.toml"),
            "schema = 1\n\n[sandbox]\nmode = \"on\"\n\n[sandbox.personas.devops]\n\
             hosts = [\"10.0.0.5:6443\", \"*.internal.example\"]\n",
        )
        .expect("toml");
        assert!(state_of(root).persona_hosts[0].waiting);
        assert!(grants_of(root, &chats)[0].waiting.is_some());
        revoke(root, &chats, &listed[0].id, &audit).expect("revoked");
        assert!(grants_of(root, &chats).is_empty());
        assert!(!state_of(root).persona_hosts[0].allowed);
        assert!(!state_of(root).persona_hosts[0].waiting);
        assert_eq!(
            *heard.lock().unwrap(),
            [
                (
                    None,
                    "trust.sandbox.grant",
                    "persona-hosts".to_owned(),
                    "devops: 10.0.0.5:6443, *.internal.example".to_owned()
                ),
                (
                    None,
                    "trust.sandbox.revoke",
                    "persona-hosts".to_owned(),
                    "devops".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn a_vault_grant_policy_forbids_is_listed_locked() {
        use sandbox::policy::{Locks, set_for_this_test};
        let project = tempfile::tempdir().expect("a project");
        let root = project.path();
        sandbox::local::grant_vault(root, "devops", "steward").expect("granted");
        set_for_this_test(Locks::parse(
            r#"{"owner": "IT", "sandbox": {"vault-grants": false}}"#,
            std::path::Path::new("/etc/purlis/policy.json"),
        ));
        let listed = grants_of(root, &crate::chats::Chats::new());
        set_for_this_test(Locks::none());
        assert_eq!(
            listed[0].locked.as_deref(),
            Some(
                "Policy forbids allowing a persona a vault it is not tagged for. Locked by \
                 policy, set by IT in /etc/purlis/policy.json."
            )
        );
    }

    /// #1431: an entry policy locks out is kept, and a person may still take it away: Revoke
    /// works on it while the policy stands, audited as any other.
    #[test]
    fn a_grant_policy_locks_out_is_still_revoked() {
        let project = tempfile::tempdir().expect("a project");
        let root = project.path();
        let chats = crate::chats::Chats::new();
        sandbox::local::grant_write(root, std::path::Path::new("/tmp/tool-cache")).expect("kept");
        sandbox::local::grant_vault(root, "devops", "steward").expect("kept");
        let _under = Under::policy(
            r#"{"owner": "IT", "sandbox": {"write-grants": false, "vault-grants": false}}"#,
        );
        let listed = grants_of(root, &chats);
        assert_eq!(listed.len(), 2, "{listed:?}");
        assert!(listed.iter().all(|one| one.locked.is_some()), "{listed:?}");
        let heard = std::sync::Mutex::new(0);
        let audit = |_: Option<u32>, _: &sandbox::grant::Audited<'_>| {
            *heard.lock().unwrap() += 1;
            Ok(())
        };
        for one in &listed {
            revoke(root, &chats, &one.id, &audit).expect("revoked");
        }
        assert!(grants_of(root, &chats).is_empty());
        assert_eq!(*heard.lock().unwrap(), 2);
    }

    #[test]
    fn a_folder_you_were_granted_is_listed_and_revoke_takes_it_out_audited_once() {
        let project = tempfile::tempdir().expect("a project");
        let root = project.path();
        let chats = crate::chats::Chats::new();
        let folder = std::path::PathBuf::from("/tmp/tool-cache");
        sandbox::local::grant_write(root, &folder).expect("granted");
        sandbox::local::record_made(
            root,
            sandbox::local::Made {
                what: "write".to_owned(),
                target: "/tmp/tool-cache".to_owned(),
                level: "you".to_owned(),
                at: 42,
                chat: None,
            },
        )
        .expect("recorded");
        let listed = grants_of(root, &chats);
        assert_eq!(
            listed,
            [SandboxGrant {
                id: "you\u{1f}write\u{1f}/tmp/tool-cache".to_owned(),
                what: GrantWhat::Write,
                target: "/tmp/tool-cache".to_owned(),
                persona: None,
                level: GrantLevel::You,
                by: None,
                at: Some(42),
                chat: None,
                locked: None,
                waiting: None,
                for_no_persona: false,
            }]
        );
        // Not audited, not revoked.
        assert!(revoke(root, &chats, &listed[0].id, &no_audit()).is_err());
        assert_eq!(grants_of(root, &chats).len(), 1);

        let heard = std::sync::Mutex::new(Vec::new());
        let audit = |number: Option<u32>, audited: &sandbox::grant::Audited<'_>| {
            heard
                .lock()
                .unwrap()
                .push((number, audited.kind(), audited.target.to_owned()));
            Ok(())
        };
        revoke(root, &chats, &listed[0].id, &audit).expect("revoked");
        assert!(grants_of(root, &chats).is_empty());
        assert!(sandbox::local::granted_writes(root).is_empty());
        // Revoked twice: the second is refused before anything is audited.
        let again = revoke(root, &chats, &listed[0].id, &audit).expect_err("not there");
        assert!(again.contains("no longer there"), "{again}");
        assert!(revoke(root, &chats, "nonsense", &audit).is_err());
        assert_eq!(
            *heard.lock().unwrap(),
            [(None, "trust.sandbox.revoke", "/tmp/tool-cache".to_owned())],
            "one audit, for the one revoke that happened"
        );
    }

    #[test]
    fn a_folder_you_list_widens_the_allowlist_and_nothing_a_class_or_path_holds() {
        let project = tempfile::tempdir().expect("a project");
        let root = project.path();
        assert_eq!(
            list_folder(root, &machine(), "/noopt/tools"),
            Ok(GrantableListed {
                folders: vec!["/noopt/tools".to_owned()],
                holds: Vec::new(),
            })
        );
        for refused in ["/", "/nohome/dev", "relative"] {
            assert!(list_folder(root, &machine(), refused).is_err(), "{refused}");
        }
        // Listed, and Settings is told what it holds that later code loads (review R3).
        let agents =
            list_folder(root, &machine(), "/nohome/dev/Library/LaunchAgents").expect("listed");
        assert!(
            agents
                .holds
                .iter()
                .any(|one| one.ends_with("Library/LaunchAgents")),
            "{agents:?}"
        );
        // One that holds a harness's own is refused outright.
        assert!(list_folder(root, &machine(), "/nohome/dev/Library").is_err());
        assert_eq!(
            listed_folders(root),
            ["/noopt/tools", "/nohome/dev/Library/LaunchAgents"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_listed_folder_that_now_resolves_elsewhere_is_dropped_and_said() {
        // Review R8: listed, then swapped for a link by whatever could write where it was.
        let project = tempfile::tempdir().expect("a project");
        let root = project.path().canonicalize().expect("real");
        let listed = root.join("tools");
        let out = root.join("elsewhere");
        std::fs::create_dir_all(&listed).expect("made");
        std::fs::create_dir_all(&out).expect("made");
        sandbox::local::list_grantable(&root, &listed).expect("listed");
        assert_eq!(listed_and_pruned(&root).dropped, Vec::<String>::new());
        std::fs::remove_dir(&listed).expect("removed");
        std::os::unix::fs::symlink(&out, &listed).expect("linked");
        let now = listed_and_pruned(&root);
        assert_eq!(now.dropped, [listed.display().to_string()]);
        assert!(now.folders.is_empty());
    }

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
                locked: None,
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

    /// #1340: Settings › Sandbox draws the core's presets, each with the hosts the core lets a
    /// chat in this project reach, the project's forges among code hosting's.
    #[test]
    fn settings_is_handed_every_preset_with_the_hosts_the_core_lists() {
        let plane = sandbox::Plane::of(Some(
            "[sandbox]\nmode = \"on\"\n\n[[forge]]\nkind = \"gitlab\"\nhost = \"git.example.org\"\n",
        ));
        let shown = presets_of(&plane, &sandbox::policy::Locks::none());
        assert_eq!(
            shown
                .iter()
                .map(|one| (one.word.as_str(), one.title.as_str()))
                .collect::<Vec<_>>(),
            [
                ("model-providers", "AI providers"),
                ("forge", "Code hosting"),
                ("toolchains", "Package registries"),
            ]
        );
        for (one, preset) in shown.iter().zip(sandbox::Preset::ALL) {
            assert_eq!(
                one.hosts,
                sandbox::hosts(&[preset], &plane, &sandbox::policy::Locks::none())
            );
        }
        assert!(shown[1].hosts.contains(&"git.example.org".to_owned()));
        assert!(!shown[0].hosts.contains(&"git.example.org".to_owned()));
        // #1422: the core says which preset widens the package caches.
        assert_eq!(
            shown
                .iter()
                .map(|one| one.widens_caches)
                .collect::<Vec<_>>(),
            [false, false, true]
        );
    }

    /// #1340, #1362: a persona's own hosts, as the core read them, and whether the person
    /// allowed them here (D-1362-7).
    #[test]
    fn settings_is_handed_each_personas_own_hosts() {
        let root = tempfile::tempdir().expect("a project");
        let plane = sandbox::Plane::of(Some(
            "[sandbox]\nmode = \"on\"\n\n[sandbox.personas.devops]\nhosts = [\"10.0.0.5:6443\"]\n",
        ));
        let none = sandbox::policy::Locks::none();
        let digest = sandbox::persona::digest(
            &[sandbox::hosts::Host::parse("10.0.0.5:6443").expect("a host")],
            false,
        );
        let devops = |allowed: bool| PersonaHosts {
            persona: "devops".to_owned(),
            hosts: vec!["10.0.0.5:6443".to_owned()],
            reached: vec!["10.0.0.5:6443".to_owned()],
            digest: digest.clone(),
            allowed,
            default: false,
            waiting: false,
        };
        assert_eq!(
            persona_hosts_of(root.path(), &plane, &none),
            [devops(false)]
        );
        sandbox::local::allow_persona_hosts(root.path(), "devops", &digest).expect("kept");
        assert_eq!(persona_hosts_of(root.path(), &plane, &none), [devops(true)]);
        assert!(persona_hosts_of(root.path(), &sandbox::Plane::of(None), &none).is_empty());
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
                presets_changed: None,
                presets: presets_of(
                    &sandbox::Plane::read(project.path()),
                    &sandbox::policy::Locks::none(),
                ),
                persona_hosts: Vec::new(),
                besides: SandboxBesides::default(),
                policy: None,
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
                presets_changed: None,
                presets: presets_of(
                    &sandbox::Plane::read(project.path()),
                    &sandbox::policy::Locks::none(),
                ),
                persona_hosts: Vec::new(),
                besides: SandboxBesides::default(),
                policy: None,
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
                presets_changed: None,
                presets: presets_of(
                    &sandbox::Plane::read(project.path()),
                    &sandbox::policy::Locks::none(),
                ),
                persona_hosts: Vec::new(),
                besides: SandboxBesides::default(),
                policy: None,
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

    /// #1385: a teammate is told once of a preset that widens, and not again once it was read.
    #[test]
    fn a_widening_preset_is_told_once_and_not_again_once_read() {
        let project = tempfile::tempdir().expect("a project");
        std::fs::write(
            project.path().join("charter.toml"),
            "[sandbox]\nmode = \"on\"\ncertificate-checks = true\n",
        )
        .expect("toml");
        let told = state_of(project.path()).presets_changed.expect("told");
        assert_eq!(told.added, ["Certificate checks"]);
        assert_eq!(told.widens.len(), 1);

        let after = acknowledge_presets(project.path(), &told.now).expect("read");

        assert_eq!(after.presets_changed, None);
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
