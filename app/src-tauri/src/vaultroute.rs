//! **A vault refused for a chat's persona offers a way forward** (#1430).
//!
//! A sandboxed chat may use only the vaults tagged for the persona the app started it as
//! (`purlis_core::secrets::brokered::authorise`, D-1407-1). A chat that asks for another is
//! refused, and that used to be where it ended. Here the refusal becomes a Notice on the chat's
//! tab with two ways out, each the person's press and never the chat's:
//!
//! - **Allow** lets that persona's chats use the vault in this project on this machine
//!   ([`allow`], D-1430-1). It is kept beside the project, in the file no sandboxed chat writes
//!   (`sandbox::local`), never in the vault registry, whose shared half is committed. It is
//!   audited before it is kept, listed in Settings › Sandbox › Granted, revocable there, and
//!   forbidden where an administrator's policy says so (`vault-grants`). The next brokered run
//!   reads it, so the chat does not restart.
//! - **Keep blocked** puts the Notice away ([`Refusals::put_away`]).
//!
//! The third way, a chat opened as the vault's persona, is not a button here (D-1430-12): the
//! chat's own refusal names it, and a direct dispatch from the Notice is #1438's.
//!
//! **Nothing a chat sends reaches any of it.** The refusal is noted by the app as it refuses
//! ([`refused`]), from its own record of the chat and the registry; Allow takes the chat's
//! persona from that record again, takes only a vault this chat was in fact refused, and no
//! line on the hook channel is a grant.
//!
//! # Telling the chat, and when (D-1430-3, D-1430-11)
//!
//! Once a vault is allowed the chat is told to run its command again: one line of purlis's
//! own fixed text ([`told_allowed`]), sent as the person's message, a bracketed paste and then
//! Enter. **This is a second gated send beside smart close's** (ADR 0064, an exception to
//! ADR 0061), and it is gated the same way, by the board and never the screen ([`when`]):
//!
//! - a chat that is waiting, and asking nothing, is sent it now;
//! - a chat mid-turn has it queued until its turn ends ([`reported`]), because a line typed
//!   into a turn lands in whatever the harness is doing;
//! - a chat that is asking the person something (a permission prompt, a question), or whose
//!   state purlis does not know, is sent nothing, and the Notice says to ask the chat. **An
//!   Enter never lands on a harness's prompt**: there it would confirm the highlighted choice.

use std::sync::{Arc, Mutex, PoisonError};

use purlis_core::sandbox;
use purlis_core::secrets::brokered::{self, NotTagged};
use purlis_core::secrets::{Ctx, Env};
use purlis_core::state::State;

use crate::planes::{PlaneId, Planes};

/// The event the window is sent when a chat was refused a vault its persona is not tagged for.
pub const REFUSED: &str = "chat-vault-refused";

/// A vault a chat was refused, as the Notice on its tab shows it: names only. No key, no
/// value, and nothing of the command the chat ran.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct VaultRefused {
    pub plane: PlaneId,
    pub session: u32,
    pub vault: String,
    /// The persona the app started the chat as.
    pub persona: String,
    /// The persona the vault registry tags the vault for, where it tags one: the registry's
    /// own text, on one line.
    pub tagged_for: Option<String>,
    /// That persona, where the project defines it: one the chat's refusal told it how to
    /// dispatch to.
    pub dispatch_to: Option<String>,
    /// Why a policy forbids Allow, where one does: the Notice then offers none.
    pub locked: Option<String>,
}

/// Told each newly refused vault. The event carries its project.
pub type Teller = Arc<dyn Fn(VaultRefused) + Send + Sync + 'static>;

/// The most refused vaults one chat holds a Notice for at once: the oldest goes first.
pub const AT_MOST_PER_CHAT: usize = 5;

/// The vaults each of a project's chats was refused, until the person answers. In memory only:
/// a relaunch starts with none, and the chat's next try raises it again.
#[derive(Debug, Default)]
pub struct Refusals {
    held: Mutex<Vec<(u32, NotTagged)>>,
    /// What each chat is told once its turn ends ([`reported`]): the lines of an Allow made
    /// while it was mid-turn, oldest first.
    queued: Mutex<Vec<(u32, String)>>,
}

impl Refusals {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(u32, NotTagged)>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Notes that chat `session` was refused `not`, and answers whether that is news: one
    /// Notice per chat and vault, however often the chat tries again.
    pub fn note(&self, session: u32, not: NotTagged) -> bool {
        let mut held = self.lock();
        if held
            .iter()
            .any(|(chat, one)| *chat == session && one.vault == not.vault)
        {
            return false;
        }
        held.push((session, not));
        let mine = held.iter().filter(|(chat, _)| *chat == session).count();
        if mine > AT_MOST_PER_CHAT
            && let Some(oldest) = held.iter().position(|(chat, _)| *chat == session)
        {
            held.remove(oldest);
        }
        true
    }

    /// What chat `session` was refused, oldest first.
    pub fn of(&self, session: u32) -> Vec<NotTagged> {
        self.lock()
            .iter()
            .filter(|(chat, _)| *chat == session)
            .map(|(_, one)| one.clone())
            .collect()
    }

    /// Whether chat `session` holds a refusal of `vault`.
    pub fn holds(&self, session: u32, vault: &str) -> bool {
        self.lock()
            .iter()
            .any(|(chat, one)| *chat == session && one.vault == vault)
    }

    /// Puts chat `session`'s refusal of `vault` away: answered, or kept blocked.
    pub fn put_away(&self, session: u32, vault: &str) {
        self.lock()
            .retain(|(chat, one)| !(*chat == session && one.vault == vault));
    }

    /// Chat `session` is closed: its refusals and anything queued for it go with it, so none
    /// outlives its chat or reaches a later chat dealt the same number.
    pub fn forget(&self, session: u32) {
        self.lock().retain(|(chat, _)| *chat != session);
        self.queue().retain(|(chat, _)| *chat != session);
    }

    fn queue(&self) -> std::sync::MutexGuard<'_, Vec<(u32, String)>> {
        self.queued.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// How many lines wait for chat `session`'s turn to end.
    #[cfg(test)]
    pub fn queued_for(&self, session: u32) -> usize {
        self.queue()
            .iter()
            .filter(|(chat, _)| *chat == session)
            .count()
    }
}

/// When a line of purlis's own is sent to a chat ([`when`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sending {
    /// Now: the chat is waiting for the person, and asking nothing.
    Now,
    /// When its turn ends: it is mid-turn.
    AtTurnsEnd,
    /// Never by purlis: the chat is asking the person something, has ended, or has reported
    /// no state. The person tells it.
    Never,
}

/// **When a chat in `state` is sent a line**, by the board's answer and never the screen's, as
/// smart close's prompt is (ADR 0064). `asking` is the board's: a permission prompt or a
/// question is open, where an Enter would answer it.
pub fn when(state: State, asking: bool) -> Sending {
    match state {
        _ if asking => Sending::Never,
        State::Waiting => Sending::Now,
        State::Running => Sending::AtTurnsEnd,
        State::Unknown | State::Done | State::Failed => Sending::Never,
    }
}

/// What became of the line an Allow tells the chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Told {
    Sent,
    Queued,
    /// Nothing was sent, and nothing will be: the person tells the chat.
    NotSent,
}

/// Tells chat `session` `text`: sent now through `send`, queued in `refusals` for its turn's
/// end, or not at all ([`when`]). A write that fails is not sent, and is not tried again.
///
/// **`glance` reads the board, and it is asked under the queue's lock**, which
/// [`reported_sending`] takes before it asks the board too, as smart close's `begin` and
/// `reported` do. So a turn that ends while this runs is never missed by both: either the
/// board already says waiting here and the line is sent now, or it says running, the line is
/// queued before the lock is let go, and the turn's end, waiting on that lock, finds it.
pub fn tell(
    refusals: &Refusals,
    session: u32,
    glance: impl FnOnce() -> (State, bool),
    text: String,
    send: impl FnOnce(u32, &str) -> Result<(), String>,
) -> Told {
    let mut queue = refusals.queue();
    let glance = glance();
    match when(glance.0, glance.1) {
        Sending::Now => match send(session, &text) {
            Ok(()) => Told::Sent,
            Err(_) => Told::NotSent,
        },
        Sending::AtTurnsEnd => {
            queue.push((session, text));
            Told::Queued
        }
        Sending::Never => Told::NotSent,
    }
}

/// A report reached the project for chat `session`: what was queued for its turn's end is
/// sent once the board (`glance`, asked under the queue's lock, as [`tell`] asks it) says it
/// is waiting and asking nothing. While it runs on, or asks the person something, the lines
/// stay queued; a chat that has ended has them dropped.
pub fn reported_sending(
    refusals: &Refusals,
    session: u32,
    glance: impl FnOnce() -> (State, bool),
    mut send: impl FnMut(u32, &str) -> Result<(), String>,
) {
    let mut queue = refusals.queue();
    if !queue.iter().any(|(chat, _)| *chat == session) {
        return;
    }
    match glance() {
        (State::Waiting, false) => {}
        (State::Done | State::Failed, _) => {
            queue.retain(|(chat, _)| *chat != session);
            return;
        }
        _ => return,
    }
    let mut mine = Vec::new();
    queue.retain(|(chat, text)| {
        if *chat == session {
            mine.push(text.clone());
            false
        } else {
            true
        }
    });
    // One paste and one Enter for all of them, so a second line never lands on the turn the
    // first began.
    let _ = send(session, &mine.join("\n"));
}

/// [`reported_sending`] for chat `session` of `held`, as its hooks report.
pub fn reported(held: &crate::planes::Held, session: u32) {
    reported_sending(
        held.vault_refusals(),
        session,
        || {
            let glance = held.board().glance(session);
            (glance.state, glance.asking)
        },
        |chat, text| {
            held.chats()
                .sessions()
                .input(chat, sent_as(text).as_bytes())
        },
    );
}

/// The bytes a line is sent as: one bracketed paste, then Enter, in one write, as smart
/// close's prompt is. Nothing in the paste can submit or end it early
/// (`crate::curation::bracketed`).
pub fn sent_as(text: &str) -> String {
    format!("{}\r", crate::curation::bracketed(text))
}

/// `not` as the window shows it for chat `session` of `plane`, with what policy says of Allow
/// now.
fn shown(plane: &PlaneId, session: u32, not: NotTagged) -> VaultRefused {
    VaultRefused {
        locked: sandbox::policy::Locks::of(plane.root()).vault_grants_refused(),
        dispatch_to: brokered::dispatchable(&Ctx::new(plane.root(), Env::from_process()), &not),
        plane: plane.clone(),
        session,
        vault: not.vault,
        persona: not.persona,
        tagged_for: not
            .tagged_for
            .as_deref()
            .map(purlis_core::personas::one_line),
    }
}

/// What the app does as it refuses `asker` the vault it asked for: where the vault is
/// registered and not one the chat's persona may use, the refusal is noted and, the first
/// time, told to the window. Everything it notes is the app's record of the chat and the
/// registry's; the line gave only the vault's name, which must be one the registry holds.
///
/// Only for a chat the app started sandboxed: any other reads its vault itself, and is not
/// refused here.
pub fn refused(
    plane: &PlaneId,
    refusals: &Refusals,
    asker: &brokered::Asker,
    vault: &str,
    tell: &(dyn Fn(VaultRefused) + Send + Sync),
) {
    if asker.confines.is_none() {
        return;
    }
    let ctx = Ctx::new(&asker.root, asker.env.clone());
    let Some(not) = brokered::not_tagged(&ctx, asker.persona.as_deref(), vault) else {
        return;
    };
    if refusals.note(asker.chat, not.clone()) {
        tell(shown(plane, asker.chat, not));
    }
}

/// What a press on the Notice answered: the sentence it then says.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct VaultAnswered {
    pub said: String,
}

/// Seconds since 1970, now.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Writes the audit of a grant, and answers whether it was written.
type Audit<'a> = &'a dyn Fn(Option<u32>, &sandbox::grant::Audited<'_>) -> Result<(), String>;

/// A chat as the app recorded it: the persona it was started as, and the name its tab shows.
pub struct Chat<'a> {
    pub session: u32,
    pub persona: Option<&'a str>,
    pub name: Option<String>,
}

/// **Lets `chat`'s persona use `vault` in the project at `root` on this machine** (#1430).
///
/// - The persona is the app's record of the chat, never the window's word.
/// - Only a vault this chat was refused, and that is still registered and still not the
///   persona's: Allow follows a refusal, and nothing else can ask for it.
/// - Never past a policy lock (`vault-grants`).
/// - Audited before it is kept: an unaudited grant is no grant.
///
/// It is kept in `sandbox::local`, which the next brokered run reads: nothing restarts.
fn allow(
    root: &std::path::Path,
    refusals: &Refusals,
    chat: &Chat<'_>,
    vault: &str,
    audit: Audit<'_>,
    at: u64,
) -> Result<sandbox::local::VaultGrant, String> {
    let session = chat.session;
    if !refusals.holds(session, vault) {
        return Err(
            "purlis allowed nothing: the chat was not refused that vault. A vault is allowed \
             from the notice its refusal raises."
                .to_owned(),
        );
    }
    let ctx = Ctx::new(root, Env::from_process());
    let Some(not) = brokered::not_tagged(&ctx, chat.persona, vault) else {
        refusals.put_away(session, vault);
        return Err(
            "There is nothing to allow now: the vault is gone from this project, or the chat's \
             persona may use it already."
                .to_owned(),
        );
    };
    if let Some(why) = sandbox::policy::Locks::of(root).vault_grants_refused() {
        return Err(why);
    }
    let grant = sandbox::local::VaultGrant {
        vault: not.vault,
        persona: not.persona,
    };
    let target = grant.target();
    audit(
        Some(session),
        &sandbox::grant::Audited {
            granted: true,
            what: sandbox::local::VAULT,
            target: &target,
            level: sandbox::grant::Level::You,
        },
    )?;
    sandbox::local::grant_vault(root, &grant.vault, &grant.persona)
        .map_err(|why| format!("purlis could not keep it, so nothing was allowed: {why}"))?;
    if let Err(why) = sandbox::local::record_made(
        root,
        sandbox::local::Made {
            what: sandbox::local::VAULT.to_owned(),
            target,
            level: sandbox::grant::Level::You.word().to_owned(),
            at,
            chat: chat.name.clone(),
        },
    ) {
        // The grant stands, and is audited; only the Granted list's "when" is lost.
        tracing::warn!("purlis: a vault grant was kept without when it was made ({why})");
    }
    refusals.put_away(session, vault);
    Ok(grant)
}

/// What the chat is told once its persona is allowed a vault: the person's own message, so it
/// runs the command again with nothing typed. purlis's own fixed text around two names: the
/// persona the app recorded and a vault name the registry holds.
pub fn told_allowed(grant: &sandbox::local::VaultGrant) -> String {
    format!(
        "purlis: the person allowed chats opened as {} to use vault {} on this machine. Nothing \
         restarted. Run the command that was refused again.",
        grant.persona, grant.vault
    )
}

/// What the Notice says once a vault is allowed, by what became of the line to the chat.
///
/// **"The chat", never "this chat"** (#1538): the Notice is drawn on a tab whose chat is not
/// on screen too (a task's, `NoticeOf`), where "this chat" would read as the one in front.
/// The Notice's own sentence says which chat it is; the answer under it names none.
pub fn allowed_said(grant: &sandbox::local::VaultGrant, told: Told) -> String {
    let then = match told {
        Told::Sent => "The chat is told to run the command again.",
        Told::Queued => "The chat is told to run the command again when its turn ends.",
        Told::NotSent => "Ask the chat to run the command again.",
    };
    format!(
        "Allowed. Chats opened as {} can use vault {} in this project on this machine. {then} \
         It does not restart.",
        grant.persona, grant.vault
    )
}

/// The vaults chat `session` was refused and the person has not answered, oldest first.
#[tauri::command]
#[specta::specta]
pub fn vault_refusals(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<Vec<VaultRefused>, String> {
    let held = planes.held(&plane)?;
    Ok(held
        .vault_refusals()
        .of(session)
        .into_iter()
        .map(|not| shown(&plane, session, not))
        .collect())
}

/// **Allow** on a refused vault's Notice: lets the chat's persona use `vault` in this project
/// on this machine, audited, and tells the chat to run its command again, now or when its turn
/// ends, or leaves that to the person ([`tell`]). No restart.
#[tauri::command]
#[specta::specta]
pub fn allow_refused_vault(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    vault: String,
) -> Result<VaultAnswered, String> {
    let held = planes.held(&plane)?;
    let root = held.root().to_path_buf();
    let open = held
        .chats()
        .open_now()
        .into_iter()
        .find(|open| open.session == session)
        .ok_or_else(|| format!("purlis allowed nothing for chat {session}: it is not open."))?;
    let grant = allow(
        &root,
        held.vault_refusals(),
        &Chat {
            session,
            persona: open.persona.as_deref(),
            name: held.chats().shown_name(session),
        },
        &vault,
        &held.audit_then_record(planes.network()),
        now_secs(),
    )?;
    let told = tell(
        held.vault_refusals(),
        session,
        || {
            let glance = held.board().glance(session);
            (glance.state, glance.asking)
        },
        told_allowed(&grant),
        |chat, text| {
            held.chats()
                .sessions()
                .input(chat, sent_as(text).as_bytes())
        },
    );
    Ok(VaultAnswered {
        said: allowed_said(&grant, told),
    })
}

/// **Keep blocked** on a refused vault's Notice: puts it away, and changes nothing. The chat's
/// next try at the vault raises it again.
#[tauri::command]
#[specta::specta]
pub fn keep_vault_blocked(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    vault: String,
) -> Result<(), String> {
    planes
        .held(&plane)?
        .vault_refusals()
        .put_away(session, &vault);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project whose vault `devops` is tagged for persona `devops`, and `loose` for nobody.
    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a project");
        std::fs::write(
            dir.path().join("vaults.json"),
            serde_json::json!({ "vaults": {
                "devops": {"provider": "plain-file", "config": {"file": "d.json"}, "persona": "devops"},
                "loose": {"provider": "plain-file", "config": {"file": "l.json"}},
            }})
            .to_string(),
        )
        .expect("a registry");
        dir
    }

    fn plane(root: &std::path::Path) -> PlaneId {
        PlaneId::for_tests(root)
    }

    /// Chat 3, as the app recorded it: started sandboxed, as `persona`.
    fn asker(root: &std::path::Path, persona: Option<&str>) -> brokered::Asker {
        brokered::Asker {
            root: root.to_path_buf(),
            env: Env::of(&[]),
            chat: 3,
            persona: persona.map(str::to_owned),
            folder: None,
            confines: Some(sandbox::Confines::default()),
        }
    }

    fn steward() -> Chat<'static> {
        Chat {
            session: 3,
            persona: Some("steward"),
            name: Some("steward 1".to_owned()),
        }
    }

    fn told() -> (
        Arc<Mutex<Vec<VaultRefused>>>,
        impl Fn(VaultRefused) + Send + Sync,
    ) {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let into = Arc::clone(&heard);
        (heard, move |one| into.lock().unwrap().push(one))
    }

    type Audits = Mutex<Vec<(Option<u32>, &'static str, String, String)>>;

    fn auditing(
        heard: &Audits,
    ) -> impl Fn(Option<u32>, &sandbox::grant::Audited<'_>) -> Result<(), String> + '_ {
        |number, audited| {
            heard.lock().unwrap().push((
                number,
                audited.kind(),
                audited.what.to_owned(),
                audited.target.to_owned(),
            ));
            Ok(())
        }
    }

    #[test]
    fn a_refused_vault_raises_one_notice_however_often_the_chat_tries() {
        let project = project();
        let root = project.path();
        let refusals = Refusals::default();
        let (heard, tell) = told();
        for _ in 0..3 {
            refused(
                &plane(root),
                &refusals,
                &asker(root, Some("steward")),
                "devops",
                &tell,
            );
        }
        assert_eq!(
            *heard.lock().unwrap(),
            [VaultRefused {
                plane: plane(root),
                session: 3,
                vault: "devops".to_owned(),
                persona: "steward".to_owned(),
                tagged_for: Some("devops".to_owned()),
                // The fixture defines no persona, so there is nobody to dispatch to.
                dispatch_to: None,
                locked: None,
            }]
        );
        assert_eq!(refusals.of(3).len(), 1);
    }

    #[test]
    fn nothing_is_raised_where_there_is_nothing_to_allow() {
        let project = project();
        let root = project.path();
        let refusals = Refusals::default();
        let (heard, tell) = told();
        // Its own vault, a vault nobody registered, a chat on no persona, and a chat the app
        // did not start sandboxed, which reads its vault itself.
        refused(
            &plane(root),
            &refusals,
            &asker(root, Some("devops")),
            "devops",
            &tell,
        );
        refused(
            &plane(root),
            &refusals,
            &asker(root, Some("steward")),
            "made-up",
            &tell,
        );
        refused(&plane(root), &refusals, &asker(root, None), "devops", &tell);
        let mut unsandboxed = asker(root, Some("steward"));
        unsandboxed.confines = None;
        refused(&plane(root), &refusals, &unsandboxed, "devops", &tell);
        assert_eq!(*heard.lock().unwrap(), []);
        assert_eq!(refusals.of(3), []);
    }

    #[test]
    fn allow_is_audited_kept_on_this_machine_and_never_written_to_the_registry() {
        let project = project();
        let root = project.path();
        let registry_before = std::fs::read_to_string(root.join("vaults.json")).unwrap();
        let refusals = Refusals::default();
        let (_, tell) = told();
        refused(
            &plane(root),
            &refusals,
            &asker(root, Some("steward")),
            "devops",
            &tell,
        );
        let heard = Audits::default();
        let grant =
            allow(root, &refusals, &steward(), "devops", &auditing(&heard), 42).expect("allowed");
        assert_eq!(grant.target(), "devops for steward");
        assert_eq!(
            *heard.lock().unwrap(),
            [(
                Some(3),
                "trust.sandbox.grant",
                "vault".to_owned(),
                "devops for steward".to_owned()
            )]
        );
        assert_eq!(
            sandbox::local::granted_vaults(root),
            std::slice::from_ref(&grant)
        );
        assert_eq!(
            sandbox::local::made(root),
            [sandbox::local::Made {
                what: "vault".to_owned(),
                target: "devops for steward".to_owned(),
                level: "you".to_owned(),
                at: 42,
                chat: Some("steward 1".to_owned()),
            }]
        );
        assert_eq!(
            std::fs::read_to_string(root.join("vaults.json")).unwrap(),
            registry_before,
            "the committed registry is not edited from a Notice"
        );
        // The same chat's next run is authorised, with nothing restarted.
        let ctx = Ctx::new(root, Env::of(&[]));
        assert_eq!(brokered::authorise(&ctx, Some("steward"), "devops"), Ok(()));
        assert_eq!(refusals.of(3), [], "the Notice is answered");
        assert_eq!(
            told_allowed(&grant),
            "purlis: the person allowed chats opened as steward to use vault devops on this \
             machine. Nothing restarted. Run the command that was refused again."
        );
    }

    #[test]
    fn allow_follows_a_refusal_an_audit_and_the_apps_own_record_of_the_chat() {
        let project = project();
        let root = project.path();
        let refusals = Refusals::default();
        let heard = Audits::default();
        // No refusal was raised for this chat and vault: nothing to press, nothing allowed.
        let unasked = allow(root, &refusals, &steward(), "devops", &auditing(&heard), 1)
            .expect_err("refused");
        assert!(unasked.contains("was not refused that vault"), "{unasked}");

        let (_, tell) = told();
        refused(
            &plane(root),
            &refusals,
            &asker(root, Some("steward")),
            "devops",
            &tell,
        );
        // Not audited, not allowed.
        let unaudited = allow(
            root,
            &refusals,
            &steward(),
            "devops",
            &|_, _| Err("the event log is not open".to_owned()),
            1,
        )
        .expect_err("refused");
        assert!(unaudited.contains("event log"), "{unaudited}");
        // A chat on no persona is allowed nothing, whatever the window sends.
        let nobody = Chat {
            session: 3,
            persona: None,
            name: None,
        };
        assert!(allow(root, &refusals, &nobody, "devops", &auditing(&heard), 1).is_err());
        assert_eq!(sandbox::local::granted_vaults(root), []);
        assert_eq!(*heard.lock().unwrap(), []);
    }

    #[test]
    fn policy_forbids_the_allow_and_the_notice_says_who_locked_it() {
        use sandbox::policy::{Locks, set_for_this_test};
        let project = project();
        let root = project.path();
        let refusals = Refusals::default();
        let (heard, tell) = told();
        set_for_this_test(Locks::parse(
            r#"{"owner": "IT", "sandbox": {"vault-grants": false}}"#,
            std::path::Path::new("/etc/purlis/policy.json"),
        ));
        refused(
            &plane(root),
            &refusals,
            &asker(root, Some("steward")),
            "devops",
            &tell,
        );
        let audits = Audits::default();
        let locked = allow(root, &refusals, &steward(), "devops", &auditing(&audits), 1);
        set_for_this_test(Locks::none());
        let said = "Policy forbids allowing a persona a vault it is not tagged for. Locked by \
                    policy, set by IT in /etc/purlis/policy.json.";
        assert_eq!(heard.lock().unwrap()[0].locked.as_deref(), Some(said));
        assert_eq!(locked.expect_err("locked"), said);
        assert_eq!(*audits.lock().unwrap(), []);
        assert_eq!(sandbox::local::granted_vaults(root), []);
    }

    #[test]
    fn a_chat_holds_a_handful_of_refusals_and_keep_blocked_puts_one_away() {
        let refusals = Refusals::default();
        let not = |vault: &str| NotTagged {
            vault: vault.to_owned(),
            persona: "steward".to_owned(),
            tagged_for: None,
        };
        for n in 0..AT_MOST_PER_CHAT + 2 {
            assert!(refusals.note(3, not(&format!("v{n}"))));
        }
        assert!(refusals.note(4, not("v0")), "another chat's is its own");
        let mine: Vec<String> = refusals.of(3).into_iter().map(|one| one.vault).collect();
        assert_eq!(mine, ["v2", "v3", "v4", "v5", "v6"]);
        refusals.put_away(3, "v4");
        assert!(!refusals.holds(3, "v4"));
        assert!(refusals.holds(4, "v0"));
        assert!(
            refusals.note(3, not("v4")),
            "its next refusal is news again"
        );
    }

    #[test]
    fn a_closed_chat_takes_its_refusals_and_its_queued_lines_with_it() {
        let refusals = Refusals::default();
        let not = |vault: &str| NotTagged {
            vault: vault.to_owned(),
            persona: "steward".to_owned(),
            tagged_for: None,
        };
        refusals.note(3, not("a"));
        refusals.note(4, not("a"));
        tell(
            &refusals,
            3,
            || (State::Running, false),
            "retry".to_owned(),
            |_, _| Ok(()),
        );
        tell(
            &refusals,
            4,
            || (State::Running, false),
            "retry".to_owned(),
            |_, _| Ok(()),
        );
        refusals.forget(3);
        assert_eq!(refusals.of(3), []);
        assert_eq!(refusals.queued_for(3), 0);
        assert!(refusals.holds(4, "a"), "another chat's stay");
        assert_eq!(refusals.queued_for(4), 1);
        assert!(
            refusals.note(3, not("a")),
            "a later chat dealt the number starts clean"
        );
    }

    // ---- telling the chat, gated by the board (D-1430-11) ----

    /// What `tell` did for a chat in `state`, and every write it made.
    fn told_in(state: State, asking: bool) -> (Told, Vec<String>, usize) {
        let refusals = Refusals::default();
        let mut sent = Vec::new();
        let told = tell(
            &refusals,
            3,
            || (state, asking),
            "retry".to_owned(),
            |_, text| {
                sent.push(text.to_owned());
                Ok(())
            },
        );
        (told, sent, refusals.queued_for(3))
    }

    #[test]
    fn a_waiting_chat_is_told_now() {
        assert_eq!(when(State::Waiting, false), Sending::Now);
        assert_eq!(
            told_in(State::Waiting, false),
            (Told::Sent, vec!["retry".to_owned()], 0)
        );
    }

    #[test]
    fn a_chat_mid_turn_is_told_when_its_turn_ends_and_never_before() {
        assert_eq!(when(State::Running, false), Sending::AtTurnsEnd);
        assert_eq!(told_in(State::Running, false), (Told::Queued, vec![], 1));

        let refusals = Refusals::default();
        for text in ["one", "two"] {
            let told = tell(
                &refusals,
                3,
                || (State::Running, false),
                text.to_owned(),
                |_, _| panic!("nothing is written into a turn"),
            );
            assert_eq!(told, Told::Queued);
        }
        let sent = std::cell::RefCell::new(Vec::new());
        let send = |chat: u32, text: &str| {
            sent.borrow_mut().push((chat, text.to_owned()));
            Ok(())
        };
        // Still running: a tool's report mid-turn sends nothing.
        reported_sending(&refusals, 3, || (State::Running, false), send);
        // Its turn ended on a question for the person: still nothing.
        reported_sending(&refusals, 3, || (State::Waiting, true), send);
        // Another chat's turn ending is not this one's.
        reported_sending(&refusals, 4, || (State::Waiting, false), send);
        assert_eq!(*sent.borrow(), []);
        assert_eq!(refusals.queued_for(3), 2);
        // Waiting, and asking nothing: one write for both lines, and then nothing is left.
        reported_sending(&refusals, 3, || (State::Waiting, false), send);
        reported_sending(&refusals, 3, || (State::Waiting, false), send);
        assert_eq!(*sent.borrow(), [(3, "one\ntwo".to_owned())]);
        assert_eq!(refusals.queued_for(3), 0);
    }

    #[test]
    fn a_turn_that_ends_while_allow_is_queueing_still_sends_the_line_on_that_turns_end() {
        // The interleaving: Allow reads the board as running, and the turn ends before the
        // line is queued. The board is read under the queue's lock, which the turn's end
        // takes too, so the end waits, finds the line queued, and sends it. Read outside the
        // lock, the end would find nothing queued and the line would wait for a turn that
        // may never come.
        let refusals = Arc::new(Refusals::default());
        let sent = Arc::new(Mutex::new(Vec::new()));
        let turns_end = Mutex::new(None);
        let told = tell(
            &refusals,
            3,
            || {
                // Allow has read "running". Now the turn ends, on the hooks' own thread.
                let (refusals, heard) = (Arc::clone(&refusals), Arc::clone(&sent));
                let (began, has_begun) = std::sync::mpsc::channel();
                *turns_end.lock().unwrap() = Some(std::thread::spawn(move || {
                    began.send(()).unwrap();
                    reported_sending(
                        &refusals,
                        3,
                        || (State::Waiting, false),
                        |chat, text| {
                            heard.lock().unwrap().push((chat, text.to_owned()));
                            Ok(())
                        },
                    );
                }));
                has_begun.recv().unwrap();
                // Time for the turn's end to reach the queue, were nothing holding it back.
                std::thread::sleep(std::time::Duration::from_millis(100));
                assert_eq!(*sent.lock().unwrap(), [], "the end waits for the line");
                (State::Running, false)
            },
            "retry".to_owned(),
            |_, _| panic!("nothing is written into a turn"),
        );
        assert_eq!(told, Told::Queued);
        turns_end
            .lock()
            .unwrap()
            .take()
            .expect("the turn ended")
            .join()
            .expect("the turn's end ran");
        assert_eq!(*sent.lock().unwrap(), [(3, "retry".to_owned())]);
        assert_eq!(refusals.queued_for(3), 0);
    }

    #[test]
    fn a_chat_that_is_asking_is_sent_nothing_so_no_enter_lands_on_its_prompt() {
        // A permission prompt or a question is open: an Enter would confirm its highlighted
        // choice. Whatever the state beside it, nothing is written and nothing is queued.
        for state in [
            State::Waiting,
            State::Running,
            State::Unknown,
            State::Done,
            State::Failed,
        ] {
            assert_eq!(when(state, true), Sending::Never, "{state:?}");
            assert_eq!(
                told_in(state, true),
                (Told::NotSent, vec![], 0),
                "{state:?}"
            );
        }
    }

    #[test]
    fn a_chat_whose_state_is_unknown_or_that_has_ended_is_sent_nothing() {
        for state in [State::Unknown, State::Done, State::Failed] {
            assert_eq!(when(state, false), Sending::Never, "{state:?}");
            assert_eq!(
                told_in(state, false),
                (Told::NotSent, vec![], 0),
                "{state:?}"
            );
        }
        // A chat that ends with a line queued has it dropped, never sent to what comes next.
        let refusals = Refusals::default();
        tell(
            &refusals,
            3,
            || (State::Running, false),
            "retry".to_owned(),
            |_, _| Ok(()),
        );
        reported_sending(
            &refusals,
            3,
            || (State::Done, false),
            |_, _| panic!("an ended chat is sent nothing"),
        );
        assert_eq!(refusals.queued_for(3), 0);
    }

    #[test]
    fn a_write_the_chat_did_not_take_is_said_as_not_sent() {
        let refusals = Refusals::default();
        let told = tell(
            &refusals,
            3,
            || (State::Waiting, false),
            "retry".to_owned(),
            |_, _| Err("the pty took nothing".to_owned()),
        );
        assert_eq!(told, Told::NotSent);
        assert_eq!(refusals.queued_for(3), 0);
    }

    #[test]
    fn the_notice_says_what_became_of_the_line_and_the_line_is_one_paste_and_one_enter() {
        let grant = sandbox::local::VaultGrant {
            vault: "devops".to_owned(),
            persona: "steward".to_owned(),
        };
        let head = "Allowed. Chats opened as steward can use vault devops in this project on \
                    this machine.";
        assert_eq!(
            allowed_said(&grant, Told::Sent),
            format!("{head} The chat is told to run the command again. It does not restart.")
        );
        assert_eq!(
            allowed_said(&grant, Told::Queued),
            format!(
                "{head} The chat is told to run the command again when its turn ends. It does \
                 not restart."
            )
        );
        assert_eq!(
            allowed_said(&grant, Told::NotSent),
            format!("{head} Ask the chat to run the command again. It does not restart.")
        );
        let sent = sent_as(&told_allowed(&grant));
        assert!(
            sent.starts_with("\x1b[200~purlis: the person allowed"),
            "{sent:?}"
        );
        assert!(sent.ends_with("again.\x1b[201~\r"), "{sent:?}");
        assert_eq!(sent.matches('\r').count(), 1, "one Enter: {sent:?}");
    }
}
