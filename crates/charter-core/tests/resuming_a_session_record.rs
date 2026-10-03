//! Resuming a session from its record (SI-8d): the Sessions panel's **Resume**.
//!
//! A new chat, in the record's place, on the record's harness, given the record's conversation
//! — through the one argument builder a relaunch uses (`start::ready`), never a second one —
//! and told the record in its briefing as data. Where the conversation cannot be given (the
//! record holds no id, names no harness charter starts, or the harness could not find it), a
//! FRESH chat with the record in its briefing, and the reason said.

use std::fs;
use std::path::{Path, PathBuf};

use charter_core::active::Place;
use charter_core::harness::{Harness, SessionId};
use charter_core::reopen::Reopened;
use charter_core::sessionrecord::{self, ChatFacts, Facts, New};
use charter_core::sessionresume::{self, NotResumed};
use charter_core::{profiles, profiletrust};

const ID: &str = "0f6c2a1e-aaaa-4bbb-8ccc-123456789abc";

const BODY: &str = "## Goal\n\ng\n\n## Done\n\nd\n\n## Decisions\n\nx\n\n## Open\n\no\n\n## \
How to resume\n\nr\n";

/// A plane with a persona, a workspace, and a stand-in harness program.
struct Plane {
    dir: tempfile::TempDir,
}

impl Plane {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("charter.toml"), "").unwrap();
        fs::create_dir_all(dir.path().join("personas/steward")).unwrap();
        fs::write(
            dir.path().join("personas/steward/persona.md"),
            "# steward\n",
        )
        .unwrap();
        let ws = charter_core::workspaces::Plane::open(dir.path())
            .workspace("alpha")
            .unwrap();
        fs::create_dir_all(ws.dir()).unwrap();
        ws.scaffold_charter().unwrap();
        Self { dir }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn harness(&self) -> PathBuf {
        stand_in::program(self.root(), "harness-stand-in", "#!/bin/sh\nexit 0\n")
    }

    /// Declares `work` (this machine's default) and `other`, both claude, both approved.
    fn declares_two_claude_profiles(&self) -> &Self {
        let bin = self.harness();
        let command = format!("[{:?}]", bin.display().to_string());
        fs::write(
            self.root().join(profiles::LOCAL_FILE),
            format!(
                "[harness]\ndefault = \"work\"\n\n[harness.work]\nkind = \"claude\"\ncommand = \
                 {command}\n\n[harness.other]\nkind = \"claude\"\ncommand = {command}\n"
            ),
        )
        .unwrap();
        let set = profiles::current(self.root());
        for name in ["work", "other"] {
            let p = set.get(name).expect("declared");
            profiletrust::record_launched(self.root(), name, &profiletrust::fingerprint(p))
                .unwrap();
        }
        self
    }

    /// Declares `work` of `kind`, running the stand-in, as this machine's default, approved.
    fn declares(&self, kind: &str) -> &Self {
        let bin = self.harness();
        fs::write(
            self.root().join(profiles::LOCAL_FILE),
            format!(
                "[harness]\ndefault = \"work\"\n\n[harness.work]\nkind = {kind:?}\ncommand = \
                 [{:?}]\n",
                bin.display().to_string()
            ),
        )
        .unwrap();
        let set = profiles::current(self.root());
        let p = set.get("work").expect("declared");
        profiletrust::record_launched(self.root(), "work", &profiletrust::fingerprint(p)).unwrap();
        self
    }

    /// A record of alpha's, written through the one writer, as a chat on `harness` in
    /// `conversation` as `persona`.
    fn record(
        &self,
        harness: Option<&str>,
        conversation: Option<&str>,
        persona: Option<&str>,
    ) -> String {
        self.record_on(harness, conversation, persona, None, None)
    }

    /// [`Self::record`], by a chat the app knew the profile and the directory of.
    fn record_on(
        &self,
        harness: Option<&str>,
        conversation: Option<&str>,
        persona: Option<&str>,
        profile: Option<&str>,
        cwd: Option<&str>,
    ) -> String {
        sessionrecord::record(
            self.root(),
            &New {
                title: "Ship the widget",
                body: BODY,
                facts: &Facts {
                    place: Place::Workspace("alpha".to_owned()),
                    at: chrono::NaiveDate::from_ymd_opt(2026, 9, 28)
                        .unwrap()
                        .and_hms_opt(14, 3, 12)
                        .unwrap(),
                    chat: Some(ChatFacts {
                        number: 3,
                        name: Some("steward 3".to_owned()),
                        harness: harness.map(str::to_owned),
                        profile: profile.map(str::to_owned),
                        conversation: conversation.map(str::to_owned),
                        cwd: cwd.map(str::to_owned),
                        unsandboxed: false,
                    }),
                    persona: persona.map(str::to_owned),
                    pieces: Vec::new(),
                },
            },
        )
        .unwrap()
        .shown
    }
}

fn env_of(ready: &charter_core::start::Ready, name: &str) -> Option<String> {
    ready
        .env
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.clone())
}

#[test]
fn a_claude_record_resumes_its_conversation_through_the_relaunch_argument_builder() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("claude");
    let shown = plane.record(Some("claude"), Some(ID), Some("steward"));

    let resumed =
        sessionresume::ready(plane.root(), &shown, "steward 9", false).expect("it starts");

    assert_eq!(resumed.fresh, None);
    assert_eq!(resumed.ready.harness, Some(Harness::ClaudeCode));
    assert_eq!(resumed.ready.args, ["--resume", ID, "--name", "steward 9"]);
    assert_eq!(
        resumed.ready.how,
        Reopened::Resumed(SessionId::new(ID).unwrap())
    );
    assert_eq!(resumed.start.profile.as_deref(), Some("work"));
    assert_eq!(resumed.start.persona.as_deref(), Some("steward"));
    assert_eq!(
        resumed.ready.cwd.as_deref(),
        Some(plane.root().join("workspaces/alpha").as_path()),
        "in the record's own workspace"
    );
    assert_eq!(
        env_of(&resumed.ready, sessionrecord::RESUMING_ENV).as_deref(),
        Some(shown.as_str()),
        "the briefing is pointed at the record"
    );
}

#[test]
fn a_codex_record_resumes_by_its_subcommand() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("codex");
    let shown = plane.record(Some("codex"), Some(ID), None);

    let resumed = sessionresume::ready(plane.root(), &shown, "codex 9", false).expect("it starts");

    assert_eq!(resumed.fresh, None);
    assert_eq!(resumed.ready.args, ["resume", ID]);
}

#[test]
fn an_opencode_record_resumes_by_its_session_flag() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("opencode");
    let id = "ses_f232c39feffecxEyWLvftyLSYU";
    let shown = plane.record(Some("opencode"), Some(id), None);

    let resumed =
        sessionresume::ready(plane.root(), &shown, "opencode 9", false).expect("it starts");

    assert_eq!(resumed.fresh, None);
    assert_eq!(resumed.ready.args, ["-s", id]);
}

#[test]
fn a_record_with_no_conversation_starts_a_fresh_chat_with_the_record_and_says_why() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("claude");
    let shown = plane.record(Some("claude"), None, Some("steward"));

    let resumed =
        sessionresume::ready(plane.root(), &shown, "steward 9", false).expect("it starts");

    assert_eq!(resumed.fresh, Some(NotResumed::NoConversation));
    assert_eq!(resumed.start.resume, None);
    assert!(
        !resumed.ready.args.iter().any(|a| a == "--resume"),
        "a fresh chat"
    );
    assert_eq!(
        env_of(&resumed.ready, sessionrecord::RESUMING_ENV).as_deref(),
        Some(shown.as_str()),
        "the record still reaches the briefing"
    );
    assert!(
        NotResumed::NoConversation
            .said()
            .contains("no conversation"),
        "the reason names the missing conversation"
    );
}

#[test]
fn a_record_on_a_harness_no_profile_here_runs_starts_fresh_on_the_default_profile() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("claude");
    let shown = plane.record(Some("gemini"), Some(ID), None);

    let resumed = sessionresume::ready(plane.root(), &shown, "claude 9", false).expect("it starts");

    assert_eq!(
        resumed.fresh,
        Some(NotResumed::NoProfileFor("gemini".to_owned()))
    );
    assert_eq!(resumed.start.profile.as_deref(), Some("work"));
    assert_eq!(resumed.start.resume, None);
    assert!(!resumed.ready.args.iter().any(|a| a == ID));
}

#[test]
fn a_record_whose_harness_is_unknown_starts_fresh_on_the_default_profile() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("claude");
    let shown = plane.record(None, Some(ID), None);

    let resumed = sessionresume::ready(plane.root(), &shown, "claude 9", false).expect("it starts");

    assert_eq!(resumed.fresh, Some(NotResumed::NoHarness));
    assert_eq!(resumed.start.resume, None);
}

#[test]
fn after_the_harness_could_not_bring_the_conversation_back_the_same_record_starts_fresh() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("claude");
    let shown = plane.record(Some("claude"), Some(ID), Some("steward"));

    let resumed = sessionresume::ready(plane.root(), &shown, "steward 9", true).expect("it starts");

    assert_eq!(
        resumed.fresh,
        Some(NotResumed::HarnessLostIt {
            harness: "claude".to_owned(),
            conversation: ID.to_owned(),
        })
    );
    assert_eq!(resumed.start.resume, None);
    assert!(!resumed.ready.args.iter().any(|a| a == ID));
    assert_eq!(
        env_of(&resumed.ready, sessionrecord::RESUMING_ENV).as_deref(),
        Some(shown.as_str())
    );
}

#[test]
fn a_persona_the_plane_no_longer_has_is_not_adopted() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("claude");
    let shown = plane.record(Some("claude"), Some(ID), Some("gone"));

    let resumed = sessionresume::ready(plane.root(), &shown, "claude 9", false).expect("it starts");

    assert_eq!(resumed.start.persona, None);
    assert_eq!(resumed.fresh, None, "the conversation still comes back");
}

#[test]
fn a_record_path_that_leaves_the_sessions_directory_starts_nothing() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("claude");
    fs::write(plane.root().join("secret.md"), "x").unwrap();
    for path in [
        "sessions/../secret.md",
        "workspaces/alpha/sessions/../../../secret.md",
        "/etc/passwd",
        "workspaces/alpha/workspace.md",
    ] {
        assert!(
            sessionresume::ready(plane.root(), path, "claude 9", false).is_err(),
            "a path that is not a record's started a chat"
        );
    }
}

#[test]
fn a_plane_root_record_resumes_at_the_plane_root() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("claude");
    let shown = sessionrecord::record(
        plane.root(),
        &New {
            title: "Tidy personas",
            body: BODY,
            facts: &Facts {
                place: Place::PlaneRoot,
                at: chrono::NaiveDate::from_ymd_opt(2026, 9, 28)
                    .unwrap()
                    .and_hms_opt(8, 0, 0)
                    .unwrap(),
                chat: Some(ChatFacts {
                    number: 1,
                    name: None,
                    harness: Some("claude".to_owned()),
                    profile: None,
                    conversation: Some(ID.to_owned()),
                    cwd: None,
                    unsandboxed: false,
                }),
                persona: None,
                pieces: Vec::new(),
            },
        },
    )
    .unwrap()
    .shown;

    let resumed = sessionresume::ready(plane.root(), &shown, "claude 9", false).expect("it starts");

    assert_eq!(resumed.ready.cwd.as_deref(), Some(plane.root()));
    assert_eq!(resumed.place, Place::PlaneRoot);
}

// ---- the record's own profile and directory (SI-8e) ----------------------------------------

#[test]
fn a_record_resumes_on_the_profile_it_names_where_this_machine_still_has_it() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares_two_claude_profiles();
    let shown = plane.record_on(Some("claude"), Some(ID), None, Some("other"), None);

    let resumed = sessionresume::ready(plane.root(), &shown, "claude 9", false).expect("it starts");

    assert_eq!(
        resumed.start.profile.as_deref(),
        Some("other"),
        "the record's profile, not the project's default"
    );
    assert_eq!(resumed.fresh, None);
    assert!(
        !resumed.notes.iter().any(|n| n.contains("profile")),
        "nothing to say about the profile"
    );
}

#[test]
fn a_record_whose_profile_is_gone_resumes_on_the_guess_and_says_so() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares_two_claude_profiles();
    let shown = plane.record_on(Some("claude"), Some(ID), None, Some("retired"), None);

    let resumed = sessionresume::ready(plane.root(), &shown, "claude 9", false).expect("it starts");

    assert_eq!(
        resumed.start.profile.as_deref(),
        Some("work"),
        "the default"
    );
    assert_eq!(resumed.fresh, None, "the conversation is still given");
    assert!(
        resumed
            .notes
            .iter()
            .any(|n| n.contains("retired") && n.contains("work")),
        "the note names the profile that is gone and the one used"
    );
}

#[test]
fn a_record_that_names_no_profile_resumes_on_the_guess_and_says_so() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares_two_claude_profiles();
    let shown = plane.record(Some("claude"), Some(ID), None);

    let resumed = sessionresume::ready(plane.root(), &shown, "claude 9", false).expect("it starts");

    assert_eq!(resumed.start.profile.as_deref(), Some("work"));
    assert!(
        resumed.notes.iter().any(|n| n.contains("profile")),
        "the note says the profile was guessed"
    );
}

#[test]
fn a_record_resumes_in_the_directory_it_ran_in_where_that_is_still_in_the_plane() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("claude");
    let piece = plane.root().join("workspaces/alpha/charter/si-8e");
    fs::create_dir_all(&piece).unwrap();
    let shown = plane.record_on(
        Some("claude"),
        Some(ID),
        None,
        Some("work"),
        Some("workspaces/alpha/charter/si-8e"),
    );

    let resumed = sessionresume::ready(plane.root(), &shown, "claude 9", false).expect("it starts");

    assert_eq!(resumed.ready.cwd.as_deref(), Some(piece.as_path()));
    assert_eq!(resumed.fresh, None);
    assert!(resumed.notes.is_empty(), "nothing guessed");
}

#[test]
fn a_record_whose_directory_is_gone_resumes_in_its_workspace_and_says_so() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("claude");
    let shown = plane.record_on(
        Some("claude"),
        Some(ID),
        None,
        Some("work"),
        Some("workspaces/alpha/charter/removed"),
    );

    let resumed = sessionresume::ready(plane.root(), &shown, "claude 9", false).expect("it starts");

    assert_eq!(
        resumed.ready.cwd.as_deref(),
        Some(plane.root().join("workspaces/alpha").as_path())
    );
    assert!(
        resumed.notes.iter().any(|n| n.contains("directory")),
        "the note says where it started instead"
    );
}

#[test]
fn a_record_that_names_no_directory_resumes_in_its_workspace_and_says_so() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("claude");
    let shown = plane.record_on(Some("claude"), Some(ID), None, Some("work"), None);

    let resumed = sessionresume::ready(plane.root(), &shown, "claude 9", false).expect("it starts");

    assert_eq!(
        resumed.ready.cwd.as_deref(),
        Some(plane.root().join("workspaces/alpha").as_path())
    );
    assert!(resumed.notes.iter().any(|n| n.contains("directory")));
}

/// A record of alpha's whose `cwd:` a hand edit set to `cwd`.
fn edited_cwd(plane: &Plane, cwd: &str) -> String {
    let shown = plane.record_on(
        Some("claude"),
        Some(ID),
        None,
        Some("work"),
        Some("workspaces/alpha"),
    );
    let path = plane.root().join(&shown);
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("cwd: workspaces/alpha", &format!("cwd: {cwd}"));
    fs::write(&path, text).unwrap();
    shown
}

#[test]
fn a_directory_that_walks_out_of_the_plane_is_never_where_a_resume_starts() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("claude");
    let workspace = plane.root().join("workspaces/alpha");
    for cwd in [
        "../..",
        "workspaces/alpha/../../..",
        "/etc",
        "/",
        "workspaces/beta",
        ".",
    ] {
        let shown = edited_cwd(&plane, cwd);
        let resumed =
            sessionresume::ready(plane.root(), &shown, "claude 9", false).expect("it starts");
        assert_eq!(
            resumed.ready.cwd.as_deref(),
            Some(workspace.as_path()),
            "a directory outside the record's place was used"
        );
        fs::remove_file(plane.root().join(&shown)).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn a_directory_that_is_a_link_out_of_the_plane_is_never_where_a_resume_starts() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("claude");
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), plane.root().join("workspaces/alpha/out")).unwrap();
    let shown = edited_cwd(&plane, "workspaces/alpha/out");

    let resumed = sessionresume::ready(plane.root(), &shown, "claude 9", false).expect("it starts");

    assert_eq!(
        resumed.ready.cwd.as_deref(),
        Some(plane.root().join("workspaces/alpha").as_path()),
        "a link out of the plane was followed"
    );
    assert!(resumed.notes.iter().any(|n| n.contains("directory")));
}

#[test]
fn a_plane_root_record_resumes_in_the_plane_root_directory_it_names() {
    charter_core::unsteered!();
    let plane = Plane::new();
    plane.declares("claude");
    let shown = sessionrecord::record(
        plane.root(),
        &New {
            title: "Tidy personas",
            body: BODY,
            facts: &Facts {
                place: Place::PlaneRoot,
                at: chrono::NaiveDate::from_ymd_opt(2026, 9, 28)
                    .unwrap()
                    .and_hms_opt(8, 0, 0)
                    .unwrap(),
                chat: Some(ChatFacts {
                    number: 1,
                    name: None,
                    harness: Some("claude".to_owned()),
                    profile: Some("work".to_owned()),
                    conversation: Some(ID.to_owned()),
                    cwd: Some(".".to_owned()),
                    unsandboxed: false,
                }),
                persona: None,
                pieces: Vec::new(),
            },
        },
    )
    .unwrap()
    .shown;

    let resumed = sessionresume::ready(plane.root(), &shown, "claude 9", false).expect("it starts");

    assert_eq!(resumed.ready.cwd.as_deref(), Some(plane.root()));
    assert!(resumed.notes.is_empty(), "nothing guessed");
}
