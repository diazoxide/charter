//! What one machine keeps about a project's sandbox: the one-time offer (ruling V21 1) and the
//! opt-out count (V12, ruling V78 d). Every expected answer is written out.

use super::*;
use crate::sandbox::{Policy, Preset};

fn a_project(charter_toml: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a directory");
    std::fs::write(dir.path().join(crate::plane::MANIFEST), charter_toml).expect("charter.toml");
    dir
}

fn charter_toml(root: &Path) -> String {
    std::fs::read_to_string(root.join(crate::plane::MANIFEST)).expect("charter.toml")
}

// -------------------------------------------------------------------------------------
// The one-time offer
// -------------------------------------------------------------------------------------

#[test]
fn an_existing_project_without_the_sandbox_is_offered_it() {
    let project = a_project("schema = 1\n\n[memory]\nshare = \"local\"\n");
    assert!(offer_due(project.path()));
}

#[test]
fn a_project_charter_made_is_never_offered_what_it_already_has() {
    let project = a_project(&crate::scaffold::planefile::render("github", "acme", None));
    assert!(!offer_due(project.path()));
}

#[test]
fn a_directory_that_is_no_project_is_offered_nothing() {
    let dir = tempfile::tempdir().expect("a directory");
    assert!(!offer_due(dir.path()));
}

#[test]
fn taking_the_offer_turns_the_sandbox_on_and_keeps_every_line_the_operator_wrote() {
    let before = "schema = 1\n# the forge we use\n[[forge]]\nkind = \"github\"\n";
    let project = a_project(before);

    answer(project.path(), Answer::TurnOn).expect("answered");

    let after = charter_toml(project.path());
    assert_eq!(
        after,
        format!(
            "{before}\n[sandbox]\nmode = \"on\"\negress = [\"model-providers\", \"forge\", \"toolchains\"]\n"
        )
    );
    assert_eq!(
        Plane::read(project.path()).said().policy,
        Some(Policy {
            egress: Preset::DEFAULT.to_vec(),
            hosts: vec![],
            certificate_checks: false,
            personas: Default::default(),
        })
    );
    assert!(!offer_due(project.path()), "asked once");
}

#[test]
fn a_sandbox_table_without_a_mode_gains_the_mode_and_keeps_its_egress() {
    let project = a_project("schema = 1\n\n[sandbox]\negress = [\"forge\"]\n");

    answer(project.path(), Answer::TurnOn).expect("answered");

    assert_eq!(
        Plane::read(project.path()).said().policy,
        Some(Policy {
            egress: vec![Preset::Forge],
            hosts: vec![],
            certificate_checks: false,
            personas: Default::default(),
        })
    );
}

#[test]
fn a_sandbox_charter_cannot_edit_safely_is_refused_and_nothing_is_written() {
    let before = "schema = 1\nsandbox = { egress = [\"forge\"] }\n";
    let project = a_project(before);

    let refused = answer(project.path(), Answer::TurnOn).expect_err("refused");

    assert!(refused.to_string().contains("mode = \"on\""), "{refused}");
    assert_eq!(charter_toml(project.path()), before);
    assert!(offer_due(project.path()), "still unanswered");
}

/// A `[sandbox]` or `mode =` line inside a multi-line string is not the table. An edit that
/// lands there leaves the sandbox off, so it is refused, nothing is written, and the offer is
/// still due — never recorded as taken with the sandbox off.
#[test]
fn an_edit_that_would_land_in_a_string_and_leave_the_sandbox_off_is_refused() {
    for before in [
        "schema = 1\nnote = \"\"\"\n[sandbox]\n\"\"\"\nsandbox = { egress = [\"forge\"] }\n",
        "schema = 1\n[sandbox]\negress = \"\"\"\nmode = 1\n\"\"\"\n",
    ] {
        let project = a_project(before);

        let refused = answer(project.path(), Answer::TurnOn).expect_err(before);

        assert!(refused.to_string().contains("by hand"), "{refused}");
        assert_eq!(charter_toml(project.path()), before, "nothing written");
        assert_eq!(Plane::read(project.path()).said().policy, None);
        assert!(offer_due(project.path()), "still unanswered: {before}");
    }
}

/// A `charter.toml` charter cannot read refuses every chat (fail closed), so it is offered
/// nothing, and taking an offer never writes into one.
#[test]
fn a_charter_toml_charter_cannot_read_is_never_offered_or_written() {
    let before = "schema = 1\n[sandbox\nmode = \"on\"\n";
    let project = a_project(before);

    assert!(!offer_due(project.path()));
    assert!(answer(project.path(), Answer::TurnOn).is_err());
    assert_eq!(charter_toml(project.path()), before, "nothing written");
}

/// A `charter.toml` that is a link refuses every chat, so it is never offered the sandbox, and
/// taking the offer never writes through it to the file it names.
#[cfg(unix)]
#[test]
fn a_charter_toml_that_is_a_link_is_never_offered_or_written_through() {
    let project = tempfile::tempdir().expect("a project");
    let elsewhere = tempfile::tempdir().expect("elsewhere");
    let real = elsewhere.path().join("charter.toml");
    std::fs::write(&real, "schema = 1\n").expect("the target");
    std::os::unix::fs::symlink(&real, project.path().join(crate::plane::MANIFEST))
        .expect("the link");

    assert!(!offer_due(project.path()));
    assert!(answer(project.path(), Answer::TurnOn).is_err());
    assert_eq!(
        std::fs::read_to_string(&real).expect("the target"),
        "schema = 1\n",
        "nothing written through the link"
    );
    assert!(
        std::fs::symlink_metadata(project.path().join(crate::plane::MANIFEST))
            .expect("still there")
            .file_type()
            .is_symlink(),
        "the link is not replaced"
    );
}

/// One over the cap the sandbox reads is unreadable too: never offered, never written.
#[test]
fn a_charter_toml_over_the_cap_is_never_offered_or_written() {
    let before = format!(
        "schema = 1\n# {}\n",
        "x".repeat(crate::sandbox::PLANE_FILE_MAX)
    );
    let project = a_project(&before);

    assert!(!offer_due(project.path()));
    assert!(answer(project.path(), Answer::TurnOn).is_err());
    assert_eq!(charter_toml(project.path()), before, "nothing written");
}

#[test]
fn declining_the_offer_changes_nothing_in_the_project_and_is_never_asked_again() {
    let before = "schema = 1\n";
    let project = a_project(before);

    answer(project.path(), Answer::KeepItOff).expect("answered");

    assert_eq!(charter_toml(project.path()), before, "nothing flips on");
    assert_eq!(Plane::read(project.path()).said().policy, None);
    assert!(!offer_due(project.path()), "asked once");
}

#[test]
fn the_answer_is_kept_where_a_chat_cannot_write_it() {
    let project = a_project("schema = 1\n");
    answer(project.path(), Answer::KeepItOff).expect("answered");

    let kept = path(project.path());
    assert!(kept.is_file(), "{}", kept.display());
    // `.charter/app/` is the integrity class's: a sandboxed chat never writes it.
    assert!(kept.starts_with(project.path().join(".charter/app")));
    assert!(IN_STATE.starts_with("app/"));
}

// -------------------------------------------------------------------------------------
// The opt-out count
// -------------------------------------------------------------------------------------

#[test]
fn a_project_with_no_chats_counted_has_an_empty_tally() {
    let project = a_project("schema = 1\n");
    assert_eq!(tally(project.path()), Tally::default());
    assert_eq!(
        tally(project.path()).said(),
        "no chat has started under this project's sandbox on this machine yet"
    );
}

#[test]
fn each_new_chat_is_counted_by_how_it_started() {
    let project = a_project("schema = 1\n");
    for started in [
        Started::Sandboxed,
        Started::Sandboxed,
        Started::Sandboxed,
        Started::OptedOut,
        Started::NoBackend,
    ] {
        count(project.path(), started).expect("counted");
    }

    assert_eq!(
        tally(project.path()),
        Tally {
            sandboxed: 3,
            opted_out: 1,
            no_backend: 1,
        }
    );
}

#[test]
fn the_rate_is_the_opt_outs_among_the_chats_a_person_could_have_sandboxed() {
    let tally = Tally {
        sandboxed: 19,
        opted_out: 1,
        no_backend: 0,
    };
    assert_eq!(tally.rate_percent(), Some(5));
    assert_eq!(
        tally.said(),
        "1 of 20 chats started without the sandbox on this machine (5%); the bar is under 10%"
    );
}

#[test]
fn chats_on_a_machine_with_no_backend_are_said_apart_and_never_counted_as_a_choice() {
    let tally = Tally {
        sandboxed: 0,
        opted_out: 0,
        no_backend: 4,
    };
    assert_eq!(tally.rate_percent(), None);
    assert_eq!(
        tally.said(),
        "4 chats started without the sandbox because this machine has no sandbox backend"
    );
}

#[test]
fn a_rate_rounds_up_so_it_never_reads_under_the_bar_when_it_is_not() {
    let tally = Tally {
        sandboxed: 90,
        opted_out: 10,
        no_backend: 2,
    };
    assert_eq!(tally.rate_percent(), Some(10));
    let tally = Tally {
        sandboxed: 2,
        opted_out: 1,
        no_backend: 0,
    };
    assert_eq!(tally.rate_percent(), Some(34));
    assert_eq!(
        tally.said(),
        "1 of 3 chats started without the sandbox on this machine (34%); the bar is under 10%"
    );
}

#[test]
fn a_tally_charter_cannot_read_reads_as_empty_and_the_next_count_starts_it_again() {
    let project = a_project("schema = 1\n");
    let file = path(project.path());
    std::fs::create_dir_all(file.parent().expect("a parent")).expect("the directory");
    std::fs::write(&file, "not json").expect("written");

    assert_eq!(tally(project.path()), Tally::default());
    count(project.path(), Started::OptedOut).expect("counted");
    assert_eq!(tally(project.path()).opted_out, 1);
}

#[test]
fn counting_keeps_the_offers_answer_and_answering_keeps_the_count() {
    let project = a_project("schema = 1\n");
    count(project.path(), Started::Sandboxed).expect("counted");
    answer(project.path(), Answer::KeepItOff).expect("answered");
    count(project.path(), Started::OptedOut).expect("counted");

    assert!(!offer_due(project.path()));
    assert_eq!(
        tally(project.path()),
        Tally {
            sandboxed: 1,
            opted_out: 1,
            no_backend: 0,
        }
    );
}

// -------------------------------------------------------------------------------------
// The project's hosts: one Notice per change, on each machine (#1341)
// -------------------------------------------------------------------------------------

const HOSTS: &str = "[sandbox]\nmode = \"on\"\nhosts = [\"10.100.39.145:6443\", \"a.example\"]\n";

fn changed(added: &[&str], removed: &[&str], now: &[&str]) -> Option<HostsChange> {
    let owned = |list: &[&str]| list.iter().map(|one| (*one).to_owned()).collect();
    Some(HostsChange {
        added: owned(added),
        removed: owned(removed),
        now: owned(now),
    })
}

#[test]
fn a_teammate_is_told_once_of_the_hosts_a_project_already_has() {
    let project = a_project(HOSTS);
    let seen = hosts_changed(project.path());
    assert_eq!(
        seen,
        changed(
            &["10.100.39.145:6443", "a.example"],
            &[],
            &["10.100.39.145:6443", "a.example"]
        )
    );
    acknowledge_hosts(project.path(), &seen.expect("a change").now).expect("kept");
    assert_eq!(hosts_changed(project.path()), None, "told once");
}

#[test]
fn a_change_names_what_was_added_and_what_was_taken_away() {
    let project = a_project(HOSTS);
    acknowledge_hosts(
        project.path(),
        &["10.100.39.145:6443".to_owned(), "a.example".to_owned()],
    )
    .expect("kept");
    std::fs::write(
        project.path().join(crate::plane::MANIFEST),
        "[sandbox]\nmode = \"on\"\nhosts = [\"a.example\", \"*.b.example\"]\n",
    )
    .expect("a teammate's push");
    assert_eq!(
        hosts_changed(project.path()),
        changed(
            &["*.b.example"],
            &["10.100.39.145:6443"],
            &["a.example", "*.b.example"]
        )
    );
}

#[test]
fn what_was_acknowledged_is_what_was_shown_so_a_later_change_is_still_told() {
    let project = a_project(HOSTS);
    // Shown before the next push, and acknowledged after it.
    acknowledge_hosts(project.path(), &["a.example".to_owned()]).expect("kept");
    assert_eq!(
        hosts_changed(project.path()),
        changed(
            &["10.100.39.145:6443"],
            &[],
            &["10.100.39.145:6443", "a.example"]
        )
    );
}

#[test]
fn a_project_with_no_hosts_or_no_sandbox_tells_nothing() {
    assert_eq!(
        hosts_changed(a_project("[sandbox]\nmode = \"on\"\n").path()),
        None
    );
    // Hosts in a project whose chats are not sandboxed reach nothing yet: told once it is on.
    assert_eq!(
        hosts_changed(a_project("[sandbox]\nhosts = [\"a.example\"]\n").path()),
        None
    );
}

/// Fold-in a of the #1362 review: an older purlis told a persona's hosts with the project's, as
/// `<host> for <persona> chats`. Such an entry is no project host taken away.
#[test]
fn a_persona_host_an_older_purlis_told_is_not_said_to_be_taken_away() {
    let project = a_project(HOSTS);
    acknowledge_hosts(
        project.path(),
        &[
            "10.100.39.145:6443".to_owned(),
            "a.example".to_owned(),
            "10.0.0.5:6443 for devops chats".to_owned(),
        ],
    )
    .expect("kept");
    assert_eq!(hosts_changed(project.path()), None);
}

#[test]
fn reordering_the_hosts_is_no_change() {
    let project = a_project(HOSTS);
    acknowledge_hosts(
        project.path(),
        &["a.example".to_owned(), "10.100.39.145:6443".to_owned()],
    )
    .expect("kept");
    assert_eq!(hosts_changed(project.path()), None);
}

#[test]
fn a_vault_allowed_for_a_persona_is_kept_here_once_and_revoked_with_its_record() {
    let plane = tempfile::tempdir().expect("a directory");
    let root = plane.path();
    assert_eq!(granted_vaults(root), Vec::new());
    grant_vault(root, "devops", "steward").expect("granted");
    grant_vault(root, "devops", "steward").expect("granted again");
    grant_vault(root, "devops", "reviewer").expect("another persona");
    let steward = VaultGrant {
        vault: "devops".into(),
        persona: "steward".into(),
    };
    assert_eq!(steward.target(), "devops for steward");
    assert_eq!(
        granted_vaults(root),
        vec![
            steward.clone(),
            VaultGrant {
                vault: "devops".into(),
                persona: "reviewer".into(),
            },
        ]
    );
    record_made(
        root,
        Made {
            what: VAULT.to_owned(),
            target: steward.target(),
            level: "you".to_owned(),
            at: 7,
            chat: Some("steward 1".to_owned()),
        },
    )
    .expect("recorded");
    revoke_vault(root, "devops", "steward").expect("revoked");
    assert_eq!(
        granted_vaults(root),
        vec![VaultGrant {
            vault: "devops".into(),
            persona: "reviewer".into(),
        }]
    );
    assert_eq!(made(root), Vec::new(), "its record goes with it");
    // It is this machine's file, in the state folder no sandboxed chat writes, and never the
    // vault registry.
    assert!(path(root).starts_with(crate::names::state(root)));
    assert!(!root.join("vaults.json").exists());
}

// -------------------------------------------------------------------------------------
// The project's Internet access presets: one Notice per change, on each machine (#1385)
// -------------------------------------------------------------------------------------

fn presets(
    text: &str,
    locks: &super::super::policy::Locks,
    seen: Option<&[&str]>,
) -> Option<PresetsChange> {
    let seen: Option<Vec<String>> =
        seen.map(|seen| seen.iter().map(|one| (*one).to_owned()).collect());
    presets_change(&Plane::of(Some(text)), locks, seen.as_deref())
}

fn no_locks() -> super::super::policy::Locks {
    super::super::policy::Locks::none()
}

#[test]
fn a_project_on_the_default_presets_tells_nothing_on_first_sight() {
    assert_eq!(
        presets("[sandbox]\nmode = \"on\"\n", &no_locks(), None),
        None
    );
    assert_eq!(
        presets(
            "[sandbox]\nmode = \"on\"\negress = [\"model-providers\", \"forge\", \"toolchains\"]\n",
            &no_locks(),
            None
        ),
        None
    );
}

#[test]
fn certificate_checks_turned_on_is_told_with_what_it_widens() {
    let told = presets(
        "[sandbox]\nmode = \"on\"\ncertificate-checks = true\n",
        &no_locks(),
        None,
    )
    .expect("a widening is told");
    assert_eq!(told.added, ["Certificate checks"]);
    assert_eq!(told.removed, Vec::<String>::new());
    assert_eq!(
        told.now,
        [
            "model-providers",
            "forge",
            "toolchains",
            "certificate-checks"
        ]
    );
    assert_eq!(told.widens.len(), 1, "{told:?}");
    assert!(told.widens[0].contains("certificate"), "{told:?}");
}

#[test]
fn a_preset_turned_back_on_after_it_was_seen_off_is_told_with_the_caches_it_widens() {
    let told = presets(
        "[sandbox]\nmode = \"on\"\negress = [\"model-providers\", \"toolchains\"]\n",
        &no_locks(),
        Some(&["model-providers"]),
    )
    .expect("a widening is told");
    assert_eq!(told.added, ["Package registries"]);
    assert_eq!(told.removed, Vec::<String>::new());
    assert_eq!(told.now, ["model-providers", "toolchains"]);
    assert!(
        told.widens.iter().any(|one| one.contains("package caches")),
        "{told:?}"
    );
}

#[test]
fn a_narrowed_project_is_told_once_so_a_later_widening_is_never_missed() {
    // First sight of a project that turned presets off: told, so what is seen is recorded.
    let told = presets(
        "[sandbox]\nmode = \"on\"\negress = [\"model-providers\"]\n",
        &no_locks(),
        None,
    )
    .expect("told");
    assert_eq!(told.added, Vec::<String>::new());
    assert_eq!(told.removed, ["Code hosting", "Package registries"]);
    assert_eq!(told.widens, Vec::<String>::new());
    // Seen as shown: nothing more until it changes.
    assert_eq!(
        presets(
            "[sandbox]\nmode = \"on\"\negress = [\"model-providers\"]\n",
            &no_locks(),
            Some(&told.now.iter().map(String::as_str).collect::<Vec<_>>()),
        ),
        None
    );
}

#[test]
fn what_was_acknowledged_is_what_was_shown_and_order_is_no_change() {
    assert_eq!(
        presets(
            "[sandbox]\nmode = \"on\"\negress = [\"toolchains\", \"forge\"]\n",
            &no_locks(),
            Some(&["forge", "toolchains"]),
        ),
        None
    );
}

#[test]
fn a_preset_policy_turns_off_reaches_no_chat_and_is_not_named() {
    let locks = super::super::policy::Locks::parse(
        r#"{"sandbox": {"presets": ["model-providers"]}}"#,
        Path::new("/etc/purlis/policy.json"),
    );
    assert_eq!(
        presets(
            "[sandbox]\nmode = \"on\"\negress = [\"model-providers\", \"toolchains\"]\n",
            &locks,
            Some(&["model-providers"]),
        ),
        None
    );
}

#[test]
fn on_first_sight_a_preset_policy_turns_off_is_not_told_as_turned_off() {
    let locks = super::super::policy::Locks::parse(
        r#"{"sandbox": {"presets": ["model-providers"]}}"#,
        Path::new("/etc/purlis/policy.json"),
    );
    // The project asks for the defaults; policy lets one through. Nothing was turned off.
    assert_eq!(presets("[sandbox]\nmode = \"on\"\n", &locks, None), None);
    // A policy that requires the sandbox: a project with none says nothing either.
    let required = super::super::policy::Locks::parse(
        r#"{"sandbox": {"opt-out": false, "presets": ["model-providers"]}}"#,
        Path::new("/etc/purlis/policy.json"),
    );
    assert_eq!(presets("schema = 1\n", &required, None), None);
}

#[test]
fn a_project_whose_chats_are_not_sandboxed_tells_nothing_of_its_presets() {
    assert_eq!(
        presets("[sandbox]\ncertificate-checks = true\n", &no_locks(), None),
        None
    );
    assert_eq!(presets("schema = 1\n", &no_locks(), None), None);
}

/// Over the file: told once, and not again once acknowledged as shown.
#[test]
fn a_teammate_is_told_once_of_a_preset_change() {
    let project = a_project("[sandbox]\nmode = \"on\"\ncertificate-checks = true\n");
    let told = presets_changed(project.path()).expect("told");
    assert_eq!(told.added, ["Certificate checks"]);
    acknowledge_presets(project.path(), &told.now).expect("kept");
    assert_eq!(presets_changed(project.path()), None, "told once");
    std::fs::write(
        project.path().join(crate::plane::MANIFEST),
        "[sandbox]\nmode = \"on\"\n",
    )
    .expect("a teammate's push");
    assert_eq!(
        presets_changed(project.path()).map(|told| told.removed),
        Some(vec!["Certificate checks".to_owned()])
    );
}

// -------------------------------------------------------------------------------------
// Seen by you: what this window wrote, never a later read of the disk (#1550)
// -------------------------------------------------------------------------------------

#[test]
fn presets_this_window_wrote_are_seen_and_not_told_back() {
    let written = "[sandbox]\nmode = \"on\"\negress = [\"forge\"]\n";
    let project = a_project(written);
    presets_seen_by_you(project.path(), false, written);
    assert_eq!(presets_changed(project.path()), None);
}

#[test]
fn a_pull_that_lands_after_the_write_is_told_and_not_recorded_as_seen() {
    let written = "[sandbox]\nmode = \"on\"\negress = [\"forge\"]\n";
    // A teammate's change pulled in between the write and the record.
    let project = a_project("[sandbox]\nmode = \"on\"\negress = [\"forge\", \"toolchains\"]\n");
    presets_seen_by_you(project.path(), false, written);
    let told = presets_changed(project.path()).expect("the pulled change is told");
    assert_eq!(told.now, ["forge", "toolchains"]);

    let written = "[sandbox]\nmode = \"on\"\nhosts = [\"api.example\"]\n";
    let project = a_project("[sandbox]\nmode = \"on\"\nhosts = [\"api.example\", \"b.example\"]\n");
    hosts_seen_by_you(project.path(), false, written);
    let told = hosts_changed(project.path()).expect("the pulled hosts are told");
    assert_eq!(told.now, ["api.example", "b.example"]);
}

#[test]
fn hosts_this_window_wrote_are_seen_unless_a_change_was_waiting() {
    let written = "[sandbox]\nmode = \"on\"\nhosts = [\"api.example\"]\n";
    let project = a_project(written);
    hosts_seen_by_you(project.path(), true, written);
    assert!(
        hosts_changed(project.path()).is_some(),
        "a waiting change is still told"
    );
    hosts_seen_by_you(project.path(), false, written);
    assert_eq!(hosts_changed(project.path()), None);
}
