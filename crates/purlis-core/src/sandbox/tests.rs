//! The sandbox policy (ADR 0067): what a plane may say, and nothing it may not. Every expected
//! answer is written out.

use super::*;

fn said(text: &str) -> Said {
    Plane::of(Some(text)).said()
}

fn default_policy() -> Option<Policy> {
    Some(Policy {
        egress: vec![Preset::ModelProviders, Preset::Forge, Preset::Toolchains],
        hosts: vec![],
        certificate_checks: false,
        personas: Default::default(),
    })
}

// -------------------------------------------------------------------------------------
// The schema: `[sandbox]` in `charter.toml`
// -------------------------------------------------------------------------------------

#[test]
fn a_plane_that_says_nothing_runs_its_chats_as_it_always_has() {
    let said = said("schema = 1\n");
    assert_eq!(said.policy, None);
    assert!(said.refused.is_empty(), "{:?}", said.refused);
}

#[test]
fn a_plane_that_turns_the_sandbox_on_gets_the_default_egress() {
    let said = said("[sandbox]\nmode = \"on\"\n");
    assert_eq!(said.policy, default_policy());
    assert!(said.refused.is_empty(), "{:?}", said.refused);
}

#[test]
fn a_plane_can_never_carry_off_and_saying_it_leaves_the_sandbox_on() {
    let said = said("[sandbox]\nmode = \"off\"\n");
    assert_eq!(said.policy, default_policy());
    assert_eq!(said.refused, [Refusal::ModeOff]);
    assert_eq!(
        said.refused[0].to_string(),
        "sandbox.mode in charter.toml cannot be \"off\": a committed file may turn the sandbox \
         on and never off — only a person turns it off, for one chat; so the sandbox is on"
    );
}

#[test]
fn a_mistyped_mode_turns_the_sandbox_on_rather_than_leaving_it_off() {
    for mode in ["\"onn\"", "\"On\"", "true", "0"] {
        let said = said(&format!("[sandbox]\nmode = {mode}\n"));
        assert_eq!(said.policy, default_policy(), "{mode}");
        assert_eq!(said.refused, [Refusal::ModeUnknown], "{mode}");
    }
}

#[test]
fn a_sandbox_that_is_not_a_table_turns_it_on() {
    let said = said("sandbox = \"off\"\n");
    assert_eq!(said.policy, default_policy());
    assert_eq!(said.refused, [Refusal::NotATable]);
}

#[test]
fn a_plane_names_its_egress_by_preset_and_an_unknown_preset_is_refused() {
    let said = said("[sandbox]\nmode = \"on\"\negress = [\"forge\", \"everywhere\"]\n");
    assert_eq!(
        said.policy,
        Some(Policy {
            egress: vec![Preset::Forge],
            hosts: vec![],
            certificate_checks: false,
            personas: Default::default(),
        })
    );
    assert_eq!(
        said.refused
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        [
            "sandbox.egress in charter.toml names \"everywhere\", which is not a preset — one of: \
          model-providers, forge, toolchains"
        ]
    );
}

#[test]
fn an_empty_egress_list_is_the_strictest_answer_and_is_kept() {
    let said = said("[sandbox]\nmode = \"on\"\negress = []\n");
    assert_eq!(
        said.policy,
        Some(Policy {
            egress: vec![],
            hosts: vec![],
            certificate_checks: false,
            personas: Default::default(),
        })
    );
}

#[test]
fn a_key_the_schema_does_not_have_is_refused() {
    let said = said("[sandbox]\nmode = \"on\"\nwritable = [\"/\"]\n");
    assert_eq!(said.refused, [Refusal::UnknownKey("writable".to_owned())]);
    assert!(said.policy.is_some());
}

#[test]
fn this_machine_s_file_holds_its_own_hosts_and_never_the_mode() {
    let off = "[sandbox]\nmode = \"off\"\n";
    assert_eq!(refusals(off, "charter.toml").len(), 1);
    assert_eq!(
        refusals(off, "charter.local.toml"),
        [
            "sandbox.mode in charter.local.toml is not read — this machine's [sandbox] holds hosts, \
          and only hosts. Whether chats run sandboxed, and the presets, are the project's, in \
          charter.toml."
        ]
    );
    let mine = "[sandbox]\nhosts = [\"10.100.39.145:6443\"]\n";
    assert_eq!(refusals(mine, "charter.local.toml"), Vec::<String>::new());
    let bad = "[sandbox]\nhosts = [\"127.0.0.1\"]\n";
    let said = refusals(bad, "charter.local.toml");
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].starts_with(
            "sandbox.hosts in charter.local.toml names \"127.0.0.1\", which no chat is let reach: \
             127.0.0.1 is this machine."
        ),
        "{said:?}"
    );
    // Any other file says nothing about the sandbox.
    assert_eq!(refusals(off, "workspace.json"), Vec::<String>::new());
}

#[test]
fn a_project_names_its_own_hosts_and_a_host_that_is_not_one_is_refused_and_dropped() {
    let said = said(
        "[sandbox]\nmode = \"on\"\nhosts = [\"10.100.39.145:6443\", \"*.internal.example\", \
         \"https://x.example/\", 7]\n",
    );
    assert_eq!(
        said.policy.expect("on").hosts,
        [
            hosts::Host::parse("10.100.39.145:6443").unwrap(),
            hosts::Host::parse("*.internal.example").unwrap(),
        ]
    );
    let refused: Vec<String> = said.refused.iter().map(ToString::to_string).collect();
    assert_eq!(
        refused,
        [
            "sandbox.hosts in charter.toml names \"https://x.example/\", which no chat is let \
             reach: That is a URL. Type its host alone, such as x.example, without the scheme or \
             a path.",
            "sandbox.hosts in charter.toml names 7, which no chat is let reach: A host is written \
             as text in quotes.",
        ]
    );
}

#[test]
fn hosts_that_are_not_a_list_grant_nothing() {
    let said = said("[sandbox]\nmode = \"on\"\nhosts = \"10.0.0.5\"\n");
    assert_eq!(said.refused, [Refusal::HostsNotAList]);
    assert_eq!(said.policy.expect("on").hosts, []);
}

// -------------------------------------------------------------------------------------
// The denial classes (ADR 0067 §5): each one on its own, and none a plane can remove
// -------------------------------------------------------------------------------------

/// A plane on a machine whose home is `/home/op`, with `vaults` as its committed registry.
fn denied_with(vaults: Option<&str>, os: Os) -> (tempfile::TempDir, Denied) {
    let plane = tempfile::tempdir().expect("a plane");
    if let Some(vaults) = vaults {
        std::fs::write(plane.path().join("vaults.json"), vaults).expect("the registry");
    }
    let denied = Denied::of(plane.path(), &machine(os));
    (plane, denied)
}

fn paths(denied: &Denied, class: Class, access: Access) -> Vec<std::path::PathBuf> {
    denied
        .paths
        .iter()
        .filter(|it| it.class == class && it.access == access)
        .map(|it| it.path.clone())
        .collect()
}

#[test]
fn the_five_classes_are_the_adrs_five() {
    assert_eq!(
        Class::ALL.map(Class::word),
        [
            "vaults",
            "integrity",
            "human-powers",
            "runner-internals",
            "later-code"
        ]
    );
}

#[test]
fn a_chat_never_reads_or_writes_a_vaults_storage() {
    let (plane, denied) = denied_with(
        Some(
            r#"{"vaults": {"dev": {"provider": "plain-file", "config": {"file": "secrets/dev.json"}}}}"#,
        ),
        Os::Linux,
    );
    let root = plane.path();
    assert_eq!(
        paths(&denied, Class::Vaults, Access::ReadWrite),
        [
            root.join(".charter/vaults"),
            root.join(".charter/fingerprint.key"),
            root.join("secrets/dev.json"),
            std::path::PathBuf::from("/home/op/.config/op"),
            std::path::PathBuf::from("/home/op/.op"),
            std::path::PathBuf::from("/home/op/.vault-token"),
        ]
    );
}

#[test]
fn a_sandboxed_chat_may_read_and_run_charters_git_hooks_and_never_rewrite_them() {
    // SQ-16, ADR 0074: git treats a hook it cannot read or run as absent and commits unscanned,
    // so nothing a chat is denied may cover the directory. The directory is the app's data
    // directory on macOS (Tauri's, under the bundle's identifier) — outside the chat's own
    // directory, which is all Claude Code's sandbox lets a command write — so a chat reads and
    // runs it and cannot rewrite it.
    let (_plane, denied) = denied_with(None, Os::MacOs);
    let hooks =
        std::path::PathBuf::from("/home/op/Library/Application Support/dev.charter.app/git-hooks");
    for denial in &denied.paths {
        assert!(
            denial.access == Access::Write || !hooks.starts_with(&denial.path),
            "{} is denied to a chat's reads and holds purlis's git hooks",
            denial.path.display()
        );
    }
    let settings = claude::settings(&compiled(denied, Os::MacOs)).expect("compiles");
    let deny_read = settings.sandbox["filesystem"]["denyRead"].to_string();
    assert!(!deny_read.contains("Library"), "{deny_read}");
}

#[test]
fn a_chat_may_read_the_registry_that_names_vaults_but_never_rewrite_it() {
    let (plane, denied) = denied_with(None, Os::Linux);
    let root = plane.path();
    assert_eq!(
        paths(&denied, Class::Vaults, Access::Write),
        [root.join("vaults.json"), root.join(".charter/vaults.json")]
    );
}

#[test]
fn a_keyring_vault_is_denied_as_the_operating_systems_credential_service() {
    let (_plane, denied) = denied_with(
        Some(r#"{"vaults": {"dev": {"provider": "keyring"}}}"#),
        Os::MacOs,
    );
    assert_eq!(denied.services, [Service::CredentialStore]);
}

#[test]
fn a_plane_with_no_keyring_vault_asks_nothing_of_the_credential_service() {
    let (_plane, denied) = denied_with(
        Some(r#"{"vaults": {"dev": {"provider": "plain-file", "config": {"file": "d.json"}}}}"#),
        Os::MacOs,
    );
    assert_eq!(denied.services, []);
}

#[test]
fn a_registry_charter_cannot_read_is_taken_to_hold_a_keyring_vault() {
    // Fail closed: a corrupt registry could be hiding the default provider.
    let (_plane, denied) = denied_with(Some("not json"), Os::MacOs);
    assert_eq!(denied.services, [Service::CredentialStore]);
}

#[test]
fn a_chat_never_writes_charters_integrity_state() {
    // Under both names of the state folder (RN-2a): the purlis one is read when it is there, so
    // a chat that could make `.purlis/app` would be writing the records charter reads next.
    let (plane, denied) = denied_with(None, Os::Linux);
    assert_eq!(
        paths(&denied, Class::Integrity, Access::Write),
        [
            plane.path().join(".purlis/app"),
            plane.path().join(".purlis/harness-profiles-launched.json"),
            plane
                .path()
                .join(".purlis/harness-declarations-approved.json"),
            plane.path().join(".purlis/mcp-approved.json"),
            plane.path().join(".charter/app"),
            plane.path().join(".charter/harness-profiles-launched.json"),
            plane
                .path()
                .join(".charter/harness-declarations-approved.json"),
            plane.path().join(".charter/mcp-approved.json"),
            // Neither folder is there yet, so neither is the chat's to make (D-RN2a-7).
            plane.path().join(".purlis"),
            plane.path().join(".charter"),
        ]
    );
}

/// #1458: the persona MCP approvals are read where `$PURLIS_HOME` puts the state folder, so
/// they are held there too.
#[test]
fn the_persona_mcp_approvals_are_held_where_purlis_home_puts_them() {
    let plane = tempfile::tempdir().expect("a plane");
    let machine = Machine {
        env: crate::secrets::Env::of(&[("PURLIS_HOME", "/srv/purlis-state")]),
        home: Some(std::path::PathBuf::from("/home/op")),
        os: Os::Linux,
    };
    let denied = Denied::of(plane.path(), &machine);
    assert!(
        paths(&denied, Class::Integrity, Access::Write).contains(&std::path::PathBuf::from(
            "/srv/purlis-state/mcp-approved.json"
        )),
        "{:?}",
        denied.paths
    );
}

#[test]
fn a_chat_cannot_plant_a_purlis_state_folder_beside_charters() {
    // D-RN2a-7: a `.purlis/` made beside `.charter/` must not be the chat's to make. The one
    // that is there is not denied whole: the rows for what is inside it hold.
    let plane = tempfile::tempdir().expect("a plane");
    std::fs::create_dir(plane.path().join(".charter")).unwrap();
    let denied = Denied::of(plane.path(), &machine(Os::Linux));
    let write = paths(&denied, Class::Integrity, Access::Write);
    assert!(write.contains(&plane.path().join(".purlis")), "{write:?}");
    assert!(!write.contains(&plane.path().join(".charter")), "{write:?}");
}

#[test]
fn a_state_folder_name_held_by_a_file_or_a_link_is_still_not_the_chats_to_make() {
    // "Is not a directory", not "is absent": a committed file or a link named `.purlis` would
    // otherwise be one `rm` away from a folder the chat fills.
    let plane = tempfile::tempdir().expect("a plane");
    std::fs::create_dir(plane.path().join(".charter")).unwrap();
    std::fs::write(plane.path().join(".purlis"), "").unwrap();
    let denied = Denied::of(plane.path(), &machine(Os::Linux));
    let write = paths(&denied, Class::Integrity, Access::Write);
    assert!(write.contains(&plane.path().join(".purlis")), "{write:?}");

    std::fs::remove_file(plane.path().join(".purlis")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(plane.path().join(".charter"), plane.path().join(".purlis"))
        .unwrap();
    let denied = Denied::of(plane.path(), &machine(Os::Linux));
    let write = paths(&denied, Class::Integrity, Access::Write);
    assert!(write.contains(&plane.path().join(".purlis")), "{write:?}");
}

/// Both names of the committed manifest and of the machine's local one.
const MANIFEST_NAMES: [&str; 4] = [
    "charter.toml",
    "purlis.toml",
    "charter.local.toml",
    "purlis.local.toml",
];

#[test]
fn no_manifest_is_held_by_name_at_any_depth() {
    // ADR 0067 §5 as amended on 2026-10-06: held only where it could change a chat's
    // sandbox, so a clone's fixtures and a temp folder may carry one.
    for name in MANIFEST_NAMES {
        assert!(
            !PLANTED.iter().any(|planted| planted.path == name),
            "{name}"
        );
    }
}

#[test]
fn the_project_roots_manifests_are_denied_under_every_name_whatever_the_chats_folder() {
    let plane = tempfile::tempdir().expect("a plane");
    let denied = Denied::of(plane.path(), &machine(Os::Linux));
    for name in MANIFEST_NAMES {
        let want = plane.path().join(name);
        assert!(
            denied.paths.iter().any(|it| it.path == want
                && it.class == Class::LaterCode
                && it.access == Access::Write),
            "{name}: {:?}",
            denied.paths
        );
    }
}

/// The second acceptance line of #667, as V22a words it: a chat cannot write another chat's
/// spool. A sandboxed chat neither reads nor writes any chat's, its own included, nor the keys
/// that check them: the hooks of the harnesses charter sandboxes run outside the sandbox their
/// tools run in (ADR 0068 §6, V63).
///
/// #980: asserted in what every compiler starts from **and** in each harness's compiled form,
/// on both systems: Claude Code's own sandbox and file-tool denials, and purlis's wrap that
/// holds a Codex or opencode chat with its Seatbelt profile. On Linux neither wrap compiles,
/// so no such chat is started (fail closed) and there is no form of theirs to hold a spool.
/// The hook channel's per-user fallback directory is #951's, not this test's.
#[cfg(unix)]
#[test]
fn a_chat_never_reads_or_writes_any_chats_hook_spool_or_its_keys() {
    for os in [Os::MacOs, Os::Linux] {
        let (plane, denied) = denied_with(None, os);
        let files: Vec<std::path::PathBuf> = [".charter", ".purlis"]
            .iter()
            .flat_map(|state| {
                let spool = crate::hookwire::spool::dir_for(
                    &plane.path().join(state).join("app/hooks.sock"),
                );
                [
                    spool.join("6").join("0123456789abcdef.1.json"),
                    spool.join("6.jsonl"),
                    spool.join(crate::hookwire::spool::KEYS),
                ]
            })
            .collect();
        let under = |held: &[std::path::PathBuf], file: &std::path::Path| {
            held.iter().any(|folder| file.starts_with(folder))
        };

        // What every compiler starts from.
        let held = paths(&denied, Class::Integrity, Access::ReadWrite);
        for file in &files {
            assert!(
                under(&held, file),
                "{os:?}: {} is not under {held:?}",
                file.display()
            );
        }

        // Claude Code: its own sandbox's read and write denials, and its file tools'. Its
        // hooks run outside that sandbox.
        let settings = claude::settings(&compiled(denied.clone(), os)).expect("compiles");
        let listed = |key: &str| -> Vec<std::path::PathBuf> {
            settings.sandbox["filesystem"][key]
                .as_array()
                .expect("a list")
                .iter()
                .filter_map(|path| path.as_str().map(std::path::PathBuf::from))
                .collect()
        };
        let tools_read: Vec<std::path::PathBuf> = settings
            .deny
            .iter()
            .filter_map(|rule| rule.strip_prefix("Read(/")?.strip_suffix("/**)"))
            .map(std::path::PathBuf::from)
            .collect();
        for file in &files {
            // As the kernel names it, which is what the compiler hands the harness.
            let named = super::real(file);
            assert!(under(&listed("denyRead"), &named), "{os:?}: {named:?}");
            assert!(under(&listed("denyWrite"), &named), "{os:?}: {named:?}");
            assert!(
                under(&tools_read, &named),
                "{os:?}: {named:?}: {:?}",
                settings.deny
            );
        }

        // Codex and opencode run inside purlis's own wrap, which there is one of on macOS.
        let codex = codex::wrap(&compiled(denied.clone(), os));
        let opencode = applied_for(
            Harness::Opencode,
            &policy_of(&[], false),
            &Plane::of(None),
            plane.path(),
            &machine(os),
        );
        if os == Os::Linux {
            assert!(
                codex.is_err(),
                "codex compiles on Linux: check its spool denial here"
            );
            assert!(
                opencode.is_err(),
                "opencode compiles on Linux: check its spool denial here"
            );
            continue;
        }
        let read_write = |denied: &[Denial]| -> Vec<std::path::PathBuf> {
            denied
                .iter()
                .filter(|it| it.access == Access::ReadWrite)
                .map(|it| it.path.clone())
                .collect()
        };
        let codex = codex.expect("compiles");
        let applied = opencode.expect("compiles");
        let Form::Opencode(wrap) = applied.form() else {
            panic!("compiled for opencode");
        };
        let held_codex = read_write(&codex.denied);
        for file in &files {
            assert!(under(&held_codex, file), "codex: {}", file.display());
            assert!(
                under(&read_write(&wrap.denied), file),
                "opencode: {}",
                file.display()
            );
        }

        // And the Seatbelt profile the wrap runs them under says so, under both firmlink
        // names of the folder that holds each spool.
        let profile = seatbelt::profile(
            &codex.denied,
            &seatbelt::Own::default(),
            &plane.path().join("workspaces/alpha"),
            std::path::Path::new("/private/tmp/chat"),
            4040,
            None,
        )
        .expect("a profile");
        for file in &files {
            let folder = held_codex
                .iter()
                .find(|folder| file.starts_with(folder))
                .expect("held above");
            for named in both_firmlink_names(super::real(folder)) {
                let rule = format!(
                    "(deny file-read* file-write* (subpath \"{}\"))",
                    named.display()
                );
                assert!(profile.contains(&rule), "{rule} missing:\n{profile}");
            }
        }
    }
}

/// #1452, D-1452-11: the dispatch records hold briefs and reports, and a chat running as one
/// persona reads none written for another. Denied for reading and writing, under both names of
/// the state folder, in what every harness is compiled.
#[test]
fn every_harness_denies_a_chat_reading_or_writing_the_dispatch_records() {
    let (plane, denied) = denied_with(None, Os::MacOs);
    let stores: Vec<std::path::PathBuf> = [".charter", ".purlis"]
        .iter()
        .map(|state| plane.path().join(state).join("app").join("dispatches"))
        .collect();
    let held = paths(&denied, Class::Integrity, Access::ReadWrite);
    for store in &stores {
        assert!(held.contains(store), "{} not in {held:?}", store.display());
    }

    // Claude Code: its own sandbox's read and write denials, and its file tools'.
    let settings = claude::settings(&compiled(denied.clone(), Os::MacOs)).expect("compiles");
    let listed = |key: &str| -> Vec<String> {
        settings.sandbox["filesystem"][key]
            .as_array()
            .expect("a list")
            .iter()
            .filter_map(|path| path.as_str().map(str::to_owned))
            .collect()
    };
    for store in &stores {
        // As the kernel names it, which is what the compiler hands the harness.
        let named = super::real(store).display().to_string();
        assert!(listed("denyRead").contains(&named), "{named}");
        assert!(listed("denyWrite").contains(&named), "{named}");
        assert!(
            settings.deny.contains(&format!("Read(/{named}/**)")),
            "{named}: {:?}",
            settings.deny
        );
    }

    // Codex and opencode: purlis's own wrap, whose profile denies both.
    let codex = codex::wrap(&compiled(denied.clone(), Os::MacOs)).expect("compiles");
    for store in &stores {
        assert!(
            codex
                .denied
                .iter()
                .any(|it| it.path == *store && it.access == Access::ReadWrite),
            "{}: {:?}",
            store.display(),
            codex.denied
        );
    }
    let cwd = plane.path().join("workspaces/alpha");
    let profile = seatbelt::profile(
        &denied.paths,
        &seatbelt::Own::default(),
        &cwd,
        std::path::Path::new("/private/tmp/chat"),
        4040,
        None,
    )
    .expect("a profile");
    for store in &stores {
        for named in both_firmlink_names(super::real(store)) {
            let rule = format!(
                "(deny file-read* file-write* (subpath \"{}\"))",
                named.display()
            );
            assert!(profile.contains(&rule), "{rule} missing:\n{profile}");
        }
    }
}

/// #1457, D-1452-12: what a chat's harness said its session cost is the figure purlis shows a
/// dispatch's cost and a session's tokens from, so no sandboxed chat may write it, its own or
/// another's. It is
/// kept in the app's own folder of the state ([`crate::usage::spend_dir`]), which every harness
/// is compiled to deny a chat writing, under both names of the state folder, on both systems.
#[test]
fn every_harness_denies_a_chat_writing_what_its_harness_said_it_cost() {
    for os in [Os::MacOs, Os::Linux] {
        let (plane, denied) = denied_with(None, os);
        let stores: Vec<std::path::PathBuf> = [".charter", ".purlis"]
            .iter()
            .map(|state| {
                plane
                    .path()
                    .join(state)
                    .join("app")
                    .join(crate::usage::SPEND_DIR_NAME)
            })
            .collect();
        // The folder the figures are written to is one of them.
        assert!(stores.contains(&crate::usage::spend_dir(plane.path())));
        let writes = |it: &Denial| matches!(it.access, Access::Write | Access::ReadWrite);
        let under = |held: &[std::path::PathBuf], file: &std::path::Path| {
            held.iter().any(|folder| file.starts_with(folder))
        };

        // What every compiler starts from.
        let held: Vec<std::path::PathBuf> = denied
            .paths
            .iter()
            .filter(|it| it.class == Class::Integrity && writes(it))
            .map(|it| it.path.clone())
            .collect();
        for store in &stores {
            assert!(under(&held, store), "{os:?}: {}", store.display());
        }

        // Claude Code: its own sandbox's write denials. Its status line, the one writer, runs
        // outside that sandbox.
        let settings = claude::settings(&compiled(denied.clone(), os)).expect("compiles");
        let denied_writes: Vec<std::path::PathBuf> = settings.sandbox["filesystem"]["denyWrite"]
            .as_array()
            .expect("a list")
            .iter()
            .filter_map(|path| path.as_str().map(std::path::PathBuf::from))
            .collect();
        for store in &stores {
            let named =
                super::real(store.parent().expect("a folder")).join(crate::usage::SPEND_DIR_NAME);
            assert!(under(&denied_writes, &named), "{os:?}: {named:?}");
        }

        // Codex and opencode: purlis's own wrap, which holds their whole harness, so none of
        // theirs writes a figure (and neither reports one today). None compiles on Linux.
        let Ok(codex) = codex::wrap(&compiled(denied.clone(), os)) else {
            assert_eq!(os, Os::Linux, "codex compiles on macOS");
            continue;
        };
        let held: Vec<std::path::PathBuf> = codex
            .denied
            .iter()
            .filter(|it| writes(it))
            .map(|it| it.path.clone())
            .collect();
        for store in &stores {
            assert!(under(&held, store), "codex: {}", store.display());
        }
    }
}

/// #1507: the record of what was refused while nobody was there is in the dispatch store, and
/// is what a standing grant is offered from. **Its own path** is denied for reading and
/// writing in what each harness is compiled, on both systems, under both names of the state
/// folder, **in a project where neither the file nor its folder is there yet**: the denial is
/// compiled from the project's root, not from what a chat finds on the disk.
#[test]
fn every_harness_denies_a_chat_the_record_of_what_was_refused_while_nobody_was_there() {
    for os in [Os::MacOs, Os::Linux] {
        let (plane, denied) = denied_with(None, os);
        let files: Vec<std::path::PathBuf> = [".charter", ".purlis"]
            .iter()
            .map(|state| {
                plane
                    .path()
                    .join(state)
                    .join("app")
                    .join(crate::dispatchrecord::DIR_NAME)
                    .join(crate::dispatchaway::FILE_NAME)
            })
            .collect();
        for file in &files {
            assert!(!file.parent().expect("a folder").exists(), "not made yet");
        }
        // The file the app writes is one of them.
        assert!(files.contains(&crate::dispatchaway::path(plane.path())));
        let under = |held: &[std::path::PathBuf], file: &std::path::Path| {
            held.iter().any(|folder| file.starts_with(folder))
        };

        // What every compiler starts from.
        let held = paths(&denied, Class::Integrity, Access::ReadWrite);
        for file in &files {
            assert!(under(&held, file), "{os:?}: {}", file.display());
        }

        // Claude Code: its own sandbox's read and write denials, and its file tools'.
        let settings = claude::settings(&compiled(denied.clone(), os)).expect("compiles");
        let listed = |key: &str| -> Vec<std::path::PathBuf> {
            settings.sandbox["filesystem"][key]
                .as_array()
                .expect("a list")
                .iter()
                .filter_map(|path| path.as_str().map(std::path::PathBuf::from))
                .collect()
        };
        for file in &files {
            // As the kernel names the folder, which is what the compiler hands the harness.
            let folder = super::real(file.parent().expect("a folder"));
            let named = folder.join(crate::dispatchaway::FILE_NAME);
            assert!(under(&listed("denyRead"), &named), "{os:?}: {named:?}");
            assert!(under(&listed("denyWrite"), &named), "{os:?}: {named:?}");
            assert!(
                settings
                    .deny
                    .contains(&format!("Read(/{}/**)", folder.display())),
                "{os:?}: {named:?}: {:?}",
                settings.deny
            );
        }

        // Codex and opencode run inside purlis's own wrap, which there is one of on macOS.
        // **On Linux neither compiles, so no such chat is started** (fail closed): there is
        // no chat of theirs to read the record.
        let codex = codex::wrap(&compiled(denied.clone(), os));
        let opencode = applied_for(
            Harness::Opencode,
            &policy_of(&[], false),
            &Plane::of(None),
            plane.path(),
            &machine(os),
        );
        if os == Os::Linux {
            assert!(
                codex.is_err(),
                "codex compiles on Linux: check its denials here"
            );
            assert!(
                opencode.is_err(),
                "opencode compiles on Linux: check its denials here"
            );
            continue;
        }
        let read_write = |denied: &[Denial]| -> Vec<std::path::PathBuf> {
            denied
                .iter()
                .filter(|it| it.access == Access::ReadWrite)
                .map(|it| it.path.clone())
                .collect()
        };
        let codex = read_write(&codex.expect("compiles").denied);
        let applied = opencode.expect("compiles");
        let Form::Opencode(wrap) = applied.form() else {
            panic!("compiled for opencode");
        };
        let opencode = read_write(&wrap.denied);
        for file in &files {
            assert!(under(&codex, file), "codex: {}", file.display());
            assert!(under(&opencode, file), "opencode: {}", file.display());
        }
    }
}

/// The same, in a wrapped opencode chat's own compiled form.
#[test]
fn an_opencode_chat_is_handed_the_dispatch_records_denied_for_reading() {
    let plane = tempfile::tempdir().expect("a plane");
    let applied = applied_for(
        Harness::Opencode,
        &policy_of(&[], false),
        &Plane::of(None),
        plane.path(),
        &machine(Os::MacOs),
    )
    .expect("compiles");
    let Form::Opencode(wrap) = applied.form() else {
        panic!("compiled for opencode");
    };
    for state in [".charter", ".purlis"] {
        let store = plane.path().join(state).join("app").join("dispatches");
        assert!(
            wrap.denied
                .iter()
                .any(|it| it.path == store && it.access == Access::ReadWrite),
            "{}: {:?}",
            store.display(),
            wrap.denied
        );
    }
}

/// #1458: the person's approvals of a profile's command and of a project's harness declaration
/// are what lets purlis start that program, so no sandboxed chat writes them, nor the folder
/// the declarations are read from, even standing at the project root with a state folder of
/// its own there. Denied for writing (both are read where a chat may read), in what every
/// harness is compiled, under both names of the state folder.
#[test]
fn every_harness_denies_a_chat_writing_the_persons_harness_approvals_or_the_declarations() {
    let plane = tempfile::tempdir().expect("a plane");
    // A state folder that is there, so the folder itself is not what holds the records.
    std::fs::create_dir(plane.path().join(".purlis")).expect("a state folder");
    let records: Vec<std::path::PathBuf> = [".charter", ".purlis"]
        .iter()
        .flat_map(|state| {
            [
                crate::profiletrust::RECORD,
                crate::harness_declaration::APPROVED,
                crate::personaverbs::mcp::APPROVED_FILE,
            ]
            .map(|record| plane.path().join(state).join(record))
        })
        .collect();
    let declarations = plane.path().join(crate::harness_declaration::DIR);

    // The classes: the records are purlis's own state; the declarations are what a program run
    // later, outside any sandbox, is started from.
    let denied = Denied::of(plane.path(), &machine(Os::MacOs));
    let integrity = paths(&denied, Class::Integrity, Access::Write);
    for record in &records {
        assert!(
            integrity.contains(record),
            "{} not in {integrity:?}",
            record.display()
        );
    }
    assert!(
        paths(&denied, Class::LaterCode, Access::Write).contains(&declarations),
        "{} not held: {:?}",
        declarations.display(),
        denied.paths
    );
    // Read, never written: the start that checks them may run where the chat's reads are held.
    for held in records.iter().chain([&declarations]) {
        assert!(
            !denied
                .paths
                .iter()
                .any(|it| it.access == Access::ReadWrite && held.starts_with(&it.path)),
            "{} is denied to reads",
            held.display()
        );
    }

    let held: Vec<&std::path::PathBuf> = records.iter().chain([&declarations]).collect();
    let mut compiled_harnesses = 0;
    for harness in Harness::ALL {
        if compiler(harness).is_none() {
            continue;
        }
        compiled_harnesses += 1;
        let applied = applied_for(
            harness,
            &policy_of(&[], false),
            &Plane::of(None),
            plane.path(),
            &machine(Os::MacOs),
        )
        .expect("compiles");
        match applied.form() {
            // Claude Code: its own sandbox's write denials, and its file tools'.
            Form::ClaudeCode(settings) => {
                let deny_write: Vec<&str> = settings.sandbox["filesystem"]["denyWrite"]
                    .as_array()
                    .expect("a list")
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect();
                for path in &held {
                    let named = super::real(path).display().to_string();
                    assert!(deny_write.contains(&named.as_str()), "{named}");
                    assert!(
                        settings.deny.contains(&format!("Edit(/{named})")),
                        "{named}: {:?}",
                        settings.deny
                    );
                }
            }
            // Codex and opencode: purlis's own wrap, whose profile a chat at the root runs in.
            Form::Codex(_) | Form::Opencode(_) => {
                let wrapped = applied.form().denied().expect("a wrap");
                for path in &held {
                    assert!(
                        wrapped
                            .iter()
                            .any(|it| it.path == **path && it.access == Access::Write),
                        "{harness:?}: {} not denied",
                        path.display()
                    );
                }
                let profile = seatbelt::profile(
                    wrapped,
                    &seatbelt::Own::default(),
                    plane.path(),
                    std::path::Path::new("/private/tmp/chat"),
                    4040,
                    None,
                )
                .expect("a profile");
                for path in &held {
                    for named in both_firmlink_names(super::real(path)) {
                        let rule = format!("(deny file-write* (subpath \"{}\"))", named.display());
                        assert!(profile.contains(&rule), "{harness:?}: {rule} missing");
                    }
                }
            }
        }
    }
    assert_eq!(compiled_harnesses, 3, "Claude Code, Codex and opencode");
}

/// D-T59-19: what waits to be told to a chat on its next turn (a task's report, purlis's word
/// on a held dispatch) is left by the app and taken by purlis's hooks. **Denied for reading
/// and writing where the harness's hooks run outside its sandbox, and left as it was where
/// they run inside it**: there the hook that delivers is held to the same sandbox, and a
/// denial would cut every report off.
#[test]
fn what_waits_for_a_chat_s_next_turn_is_denied_only_where_the_hooks_run_outside_the_sandbox() {
    let plane = tempfile::tempdir().expect("a plane");
    let stores: Vec<std::path::PathBuf> = [".charter", ".purlis"]
        .iter()
        .map(|state| plane.path().join(state).join("handbacks"))
        .collect();
    // No harness is given the denial by the classes alone: it is added per harness.
    let classes = Denied::of(plane.path(), &machine(Os::MacOs));
    for store in &stores {
        assert!(!classes.paths.iter().any(|it| it.path == *store));
    }
    let mut kinds = (0, 0);
    for harness in Harness::ALL {
        if compiler(harness).is_none() {
            continue;
        }
        let applied = applied_for(
            harness,
            &policy_of(&[], false),
            &Plane::of(None),
            plane.path(),
            &machine(Os::MacOs),
        )
        .expect("compiles");
        let denies = |store: &std::path::Path| {
            applied
                .denied
                .iter()
                .any(|it| it.path == *store && it.access == Access::ReadWrite)
        };
        if harness.adapter().sandbox_holds_what_it_starts() {
            kinds.1 += 1;
            for store in &stores {
                assert!(
                    !denies(store),
                    "{harness:?} runs its hooks inside its sandbox, and they take from {}",
                    store.display()
                );
            }
        } else {
            kinds.0 += 1;
            for store in &stores {
                assert!(denies(store), "{harness:?}: {} not denied", store.display());
            }
        }
    }
    assert!(
        kinds.0 > 0 && kinds.1 > 0,
        "both kinds are compiled: {kinds:?}"
    );
}

/// The messages between an asking chat and its tasks (#1442) wait in that same store, in
/// `said-<chat>/` beside the reports, so the one rule holds them too: denied with the reports
/// where the hooks run outside the sandbox, and reachable by the hook that takes them where
/// they run inside it (D-T59-19, D-T59-j14).
#[test]
fn the_messages_between_chats_wait_inside_the_store_that_denial_names() {
    let plane = tempfile::tempdir().expect("a plane");
    let message = crate::dispatchtalk::Message {
        kind: crate::dispatchtalk::Kind::FollowUp,
        from: "steward 3".to_owned(),
        chat: 3,
        text: "Also check staging.".to_owned(),
    };
    let file = crate::dispatchtalk::leave(plane.path(), 9, &message).expect("left");
    assert!(
        file.starts_with(crate::handback::dir(plane.path()).join("said-9")),
        "{}",
        file.display()
    );
    let denied =
        Denied::of(plane.path(), &machine(Os::MacOs)).and_what_waits_for_a_chat(plane.path());
    let held = paths(&denied, Class::Integrity, Access::ReadWrite);
    assert!(
        held.iter().any(|store| file.starts_with(store)),
        "{} is under none of {held:?}",
        file.display()
    );
    // And without that rule, as on a harness whose hooks run inside its sandbox, nothing
    // denies it: the hook that takes the message there can still read and remove it.
    let classes = Denied::of(plane.path(), &machine(Os::MacOs));
    assert!(
        !classes.paths.iter().any(|it| file.starts_with(&it.path)),
        "{}",
        file.display()
    );
}

/// The same, in the form a harness whose hooks run outside is handed: its own sandbox's read
/// and write denials, and its file tools'.
#[test]
fn a_chat_whose_hooks_run_outside_is_handed_what_waits_denied_for_reading_and_writing() {
    let plane = tempfile::tempdir().expect("a plane");
    let denied =
        Denied::of(plane.path(), &machine(Os::MacOs)).and_what_waits_for_a_chat(plane.path());
    let stores: Vec<std::path::PathBuf> = [".charter", ".purlis"]
        .iter()
        .map(|state| plane.path().join(state).join("handbacks"))
        .collect();
    let held = paths(&denied, Class::Integrity, Access::ReadWrite);
    for store in &stores {
        assert!(held.contains(store), "{} not in {held:?}", store.display());
    }
    let settings = claude::settings(&compiled(denied.clone(), Os::MacOs)).expect("compiles");
    let listed = |key: &str| -> Vec<String> {
        settings.sandbox["filesystem"][key]
            .as_array()
            .expect("a list")
            .iter()
            .filter_map(|path| path.as_str().map(str::to_owned))
            .collect()
    };
    for store in &stores {
        // As the kernel names it, which is what the compiler hands the harness.
        let named = super::real(store).display().to_string();
        assert!(listed("denyRead").contains(&named), "{named}");
        assert!(listed("denyWrite").contains(&named), "{named}");
        assert!(
            settings.deny.contains(&format!("Read(/{named}/**)")),
            "{named}: {:?}",
            settings.deny
        );
    }
}

#[test]
fn a_chat_never_writes_the_approvals_a_person_gave() {
    let (_plane, denied) = denied_with(None, Os::Linux);
    assert_eq!(
        paths(&denied, Class::HumanPowers, Access::Write),
        // Under both names, there or not: whichever is there is the config home (RN-5).
        // …and this machine's policy (#1343), as the kernel names it.
        [
            std::path::PathBuf::from("/home/op/.config/purlis"),
            std::path::PathBuf::from("/home/op/.config/charter"),
            super::real(&super::policy::machine_folder()),
        ]
    );
}

#[test]
fn a_chat_never_reads_or_writes_the_forge_answers_the_humans_token_fetched() {
    // The native forge transport's ETag store holds raw answers the human's sign-in token
    // fetched (ADR 0070 §3, FW-2a); a chat reads neither it nor the bodies in it.
    let (_plane, denied) = denied_with(None, Os::Linux);
    assert_eq!(
        paths(&denied, Class::HumanPowers, Access::ReadWrite),
        [
            // The human client scopes' credentials and `charterd.sock` (FD-27), under every
            // name the folder and the config home have (RN-5).
            std::path::PathBuf::from("/home/op/.config/purlis/purlisd"),
            std::path::PathBuf::from("/home/op/.config/purlis/charterd"),
            std::path::PathBuf::from("/home/op/.config/purlis/forge-etags"),
            std::path::PathBuf::from("/home/op/.config/charter/purlisd"),
            std::path::PathBuf::from("/home/op/.config/charter/charterd"),
            std::path::PathBuf::from("/home/op/.config/charter/forge-etags"),
        ]
    );
}

#[test]
fn claude_code_and_codex_both_deny_a_chat_the_forge_etag_store() {
    let etags = "/home/op/.config/charter/forge-etags";
    let (_plane, denied) = denied_with(None, Os::Linux);
    let settings = claude::settings(&compiled(denied.clone(), Os::Linux)).expect("compiles");
    let read_denied = settings.sandbox["filesystem"]["denyRead"].clone();
    assert!(
        read_denied
            .as_array()
            .is_some_and(|all| all.iter().any(|p| p == etags)),
        "{read_denied}"
    );
    assert!(
        settings.deny.contains(&format!("Read(/{etags}/**)")),
        "{:?}",
        settings.deny
    );
    let wrap = codex::wrap(&compiled(denied, Os::MacOs)).expect("compiles");
    assert!(
        wrap.denied
            .iter()
            .any(|it| it.path == std::path::Path::new(etags) && it.access == Access::ReadWrite),
        "{:?}",
        wrap.denied
    );
}

#[test]
fn off_a_runner_there_are_no_runner_internals_to_deny() {
    // A placeholder for class 4, kept because ADR 0067 fixes the classes: charter has no
    // runner yet (RR-5), so there is nothing of one to deny. The runner slice replaces this
    // with the paths it installs, denied on a runner.
    let (_plane, denied) = denied_with(None, Os::Linux);
    assert!(
        denied
            .paths
            .iter()
            .all(|it| it.class != Class::RunnerInternals)
    );
}

// -------------------------------------------------------------------------------------
// Egress: named presets, and the plane's own forge hosts
// -------------------------------------------------------------------------------------

fn hosts_of(presets: &[Preset], plane: Option<&str>) -> Vec<String> {
    hosts(presets, &Plane::of(plane), &policy::Locks::none())
}

#[test]
fn each_preset_has_the_name_settings_shows_it_by() {
    let titles: Vec<&str> = Preset::ALL.iter().map(|preset| preset.title()).collect();
    assert_eq!(
        titles,
        ["AI providers", "Code hosting", "Package registries"]
    );
}

#[test]
fn settings_counts_the_hosts_a_chat_is_granted_besides_its_presets() {
    let host = |written: &str| hosts::Host::parse(written).expect("a host");
    let counted = counted_hosts(&hosts::off_this_machine(
        hosts::in_force(
            &[host("api.example.com"), host("10.0.0.5")],
            &[host("api.example.com"), host("tools.example.org")],
            &[],
            &hosts::Locks::default(),
        ),
        &["10.0.0.5".parse().expect("an address")],
    ));
    // This machine's own address reaches nothing, and a host yours repeats is the project's.
    assert_eq!(
        counted,
        Besides {
            project_hosts: 1,
            your_hosts: 1,
            folders: 0,
        }
    );
}

/// #1340: what Settings counts is what a chat on no persona is granted as it starts, so the
/// sentence and a start never drift apart. Both read [`granted_hosts`] and [`granted_folders`];
/// a folder grant cannot be compiled from a fixture in a temp folder (D-1342-14), so the
/// folders compare at none here.
#[test]
fn settings_counts_what_a_chat_with_no_persona_is_granted() {
    let project = tempfile::tempdir().expect("a project");
    let root = project.path().canonicalize().expect("the project");
    let plane = Plane::of(Some(
        "[sandbox]\nmode = \"on\"\negress = []\nhosts = [\"api.example.com\", \"tools.example.org\"]\n",
    ));
    let policy = plane.said().policy.expect("on");
    let machine = Machine::this();

    let counted = besides(&root, &plane, &machine);
    let chat = Compiled::granted(
        &policy,
        &plane,
        &root,
        &machine,
        None,
        &grant::Grants::default(),
    );

    assert_eq!(
        counted.project_hosts + counted.your_hosts,
        chat.hosts.len(),
        "{:?}",
        chat.hosts
    );
    assert_eq!(counted.project_hosts, 2);
    assert_eq!(counted.folders, chat.writable.len());
}

#[test]
fn a_project_without_the_sandbox_has_nothing_besides_its_presets() {
    let root = tempfile::tempdir().expect("a project");
    assert_eq!(
        besides(root.path(), &Plane::of(None), &Machine::this()),
        Besides::default()
    );
}

#[test]
fn the_forge_preset_adds_the_self_managed_hosts_the_plane_tracks() {
    let hosts = hosts_of(
        &[Preset::Forge],
        Some("[[forge]]\nkind = \"gitlab\"\nhost = \"git.example.org:8443\"\n"),
    );
    assert!(hosts.contains(&"github.com".to_owned()), "{hosts:?}");
    assert!(hosts.contains(&"gitlab.com".to_owned()), "{hosts:?}");
    assert!(hosts.contains(&"git.example.org".to_owned()), "{hosts:?}");
    assert!(
        !hosts.contains(&"api.anthropic.com".to_owned()),
        "{hosts:?}"
    );
}

#[test]
fn a_forge_host_that_is_not_a_host_is_never_let_through() {
    let hosts = hosts_of(&[Preset::Forge], Some("[[forge]]\nhost = \"*\"\n"));
    assert!(!hosts.contains(&"*".to_owned()), "{hosts:?}");
}

#[test]
fn no_preset_is_no_host() {
    assert_eq!(hosts_of(&[], None), Vec::<String>::new());
}

#[test]
fn the_model_provider_preset_reaches_the_three_harnesses_providers() {
    let hosts = hosts_of(&[Preset::ModelProviders], None);
    for host in ["api.anthropic.com", "api.openai.com", "opencode.ai"] {
        assert!(hosts.contains(&host.to_owned()), "{host}: {hosts:?}");
    }
}

/// What each harness's compiled sandbox lets a chat reach, as its compiler hands it on: Claude
/// Code's `network.allowedDomains`, and the egress proxy's list for a harness purlis wraps.
fn reached(applied: &Applied) -> Vec<String> {
    match applied.form() {
        Form::ClaudeCode(settings) => settings.sandbox["network"]["allowedDomains"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|host| host.as_str().expect("text").to_owned())
            .collect(),
        Form::Codex(wrap) => wrap.hosts.clone(),
        Form::Opencode(wrap) => wrap.hosts.clone(),
    }
}

/// #1341: the project's own hosts reach every chat in it with no approval, on every harness,
/// private addresses and ports included; this machine's are added to them, and a host that is
/// not one grants nothing.
#[test]
fn the_project_s_hosts_and_this_machine_s_reach_a_chat_on_every_harness() {
    let plane = plane_saying(
        "[sandbox]\nmode = \"on\"\negress = []\nhosts = [\"10.100.39.145:6443\", \
         \"*.internal.example\", \"169.254.169.254\"]\n",
    );
    std::fs::write(
        plane.path().join("charter.local.toml"),
        "[sandbox]\nhosts = [\"[fd00::7]:8443\", \"10.100.39.145:6443\"]\n",
    )
    .expect("charter.local.toml");
    // Confirmed in Settings on this machine, as adding it there does.
    local::confirm_host(plane.path(), "[fd00::7]:8443").expect("confirmed");
    for harness in Harness::ALL {
        let applied =
            compiled_anyway(harness, plane.path(), &machine(Os::MacOs)).expect("compiles");
        assert_eq!(
            reached(&applied),
            ["10.100.39.145:6443", "*.internal.example", "[fd00::7]:8443"],
            "{harness:?}"
        );
    }
}

/// #1341: another project's hosts, and this machine's file in another project, reach nothing
/// here: hosts are the project's and this machine's for this project.
#[test]
fn a_project_without_hosts_reaches_only_its_presets() {
    let plane = plane_saying("[sandbox]\nmode = \"on\"\negress = []\n");
    let other = plane_saying("[sandbox]\nmode = \"on\"\nhosts = [\"10.0.0.5\"]\n");
    std::fs::write(
        other.path().join("charter.local.toml"),
        "[sandbox]\nhosts = [\"10.0.0.6\"]\n",
    )
    .expect("charter.local.toml");
    for harness in Harness::ALL {
        let applied =
            compiled_anyway(harness, plane.path(), &machine(Os::MacOs)).expect("compiles");
        assert_eq!(reached(&applied), Vec::<String>::new(), "{harness:?}");
    }
}

/// #1341: this machine's file grants nothing once git would carry it, since what it says
/// would then reach every clone with no trace of the project's flow.
#[test]
fn this_machine_s_hosts_in_a_file_git_would_commit_grant_nothing() {
    let plane = plane_saying("[sandbox]\nmode = \"on\"\negress = []\n");
    crate::testgit::run(plane.path(), &["init", "-q"]);
    std::fs::write(
        plane.path().join("charter.local.toml"),
        "[sandbox]\nhosts = [\"10.0.0.6\"]\n",
    )
    .expect("charter.local.toml");
    let applied =
        compiled_anyway(Harness::ClaudeCode, plane.path(), &machine(Os::MacOs)).expect("compiles");
    assert_eq!(reached(&applied), Vec::<String>::new());
}

// -------------------------------------------------------------------------------------
// Claude Code: the `sandbox` object in the `--settings` charter already passes
// -------------------------------------------------------------------------------------

fn compiled(denied: Denied, os: Os) -> Compiled {
    Compiled {
        denied,
        hosts: vec!["github.com".to_owned()],
        writable: Vec::new(),
        os,
        homes: Homes::default(),
        widened: Widened::default(),
    }
}

fn one(class: Class, path: &str, access: Access) -> Denial {
    Denial {
        class,
        path: std::path::PathBuf::from(path),
        access,
        named: None,
    }
}

#[test]
fn a_claude_code_chat_is_sandboxed_with_no_way_out_and_no_silent_fallback() {
    let settings = claude::settings(&compiled(Denied::default(), Os::MacOs)).expect("compiles");
    assert_eq!(settings.sandbox["enabled"], true);
    assert_eq!(settings.sandbox["allowUnsandboxedCommands"], false);
    assert_eq!(settings.sandbox["failIfUnavailable"], true);
}

#[test]
fn a_claude_code_chat_reaches_only_the_presets_hosts_and_is_never_asked_to_widen_them() {
    let settings = claude::settings(&compiled(Denied::default(), Os::MacOs)).expect("compiles");
    assert_eq!(
        settings.sandbox["network"],
        serde_json::json!({
            "allowedDomains": ["github.com"],
            "strictAllowlist": true,
            "allowAllUnixSockets": false,
        })
    );
}

#[test]
fn claude_codes_web_tools_are_denied_because_the_allowed_hosts_do_not_hold_them() {
    let settings = claude::settings(&compiled(Denied::default(), Os::MacOs)).expect("compiles");
    assert_eq!(settings.deny[..2], ["WebFetch", "WebSearch"]);
}

/// `rules` without the later-code class's, which name a path at any depth (`**/`).
fn by_path(rules: &[String]) -> Vec<String> {
    rules
        .iter()
        .filter(|rule| !rule.contains("**/"))
        .cloned()
        .collect()
}

/// A Claude Code sandbox's `denyRead` and `denyWrite`, each without the later-code class's.
fn filesystem_by_path(settings: &claude::Settings) -> (Vec<String>, Vec<String>) {
    let list = |key: &str| -> Vec<String> {
        settings.sandbox["filesystem"][key]
            .as_array()
            .expect("a list")
            .iter()
            .map(|it| it.as_str().expect("a path").to_owned())
            .collect()
    };
    (by_path(&list("denyRead")), by_path(&list("denyWrite")))
}

#[test]
fn a_path_denied_to_read_is_denied_to_the_sandbox_and_to_claude_codes_own_tools() {
    let denied = Denied {
        paths: vec![one(Class::Vaults, "/p/.charter/vaults", Access::ReadWrite)],
        services: vec![],
        unread: None,
    };
    let settings = claude::settings(&compiled(denied, Os::Linux)).expect("compiles");
    assert_eq!(
        filesystem_by_path(&settings),
        (
            vec!["/p/.charter/vaults".to_owned()],
            vec!["/p/.charter/vaults".to_owned()]
        )
    );
    assert_eq!(
        by_path(&settings.deny)[2..],
        [
            "Read(//p/.charter/vaults)",
            "Read(//p/.charter/vaults/**)",
            "Edit(//p/.charter/vaults)",
            "Edit(//p/.charter/vaults/**)",
        ]
    );
}

#[test]
fn a_path_denied_to_write_stays_readable() {
    let denied = Denied {
        paths: vec![one(Class::Integrity, "/p/.charter/app", Access::Write)],
        services: vec![],
        unread: None,
    };
    let settings = claude::settings(&compiled(denied, Os::Linux)).expect("compiles");
    assert_eq!(
        filesystem_by_path(&settings),
        (Vec::new(), vec!["/p/.charter/app".to_owned()])
    );
    assert_eq!(
        by_path(&settings.deny)[2..],
        ["Edit(//p/.charter/app)", "Edit(//p/.charter/app/**)"]
    );
}

/// A Claude Code sandbox's `denyWrite`, the later-code class's included.
fn deny_write(settings: &claude::Settings) -> Vec<String> {
    settings.sandbox["filesystem"]["denyWrite"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|it| it.as_str().expect("a path").to_owned())
        .collect()
}

#[test]
fn a_claude_code_chat_is_held_from_writing_every_later_code_name_at_any_depth() {
    let settings = claude::settings(&compiled(Denied::default(), Os::MacOs)).expect("compiles");
    let deny_write = deny_write(&settings);
    let held = |glob: &str| {
        deny_write.iter().any(|it| it == glob)
            && settings
                .deny
                .iter()
                .any(|it| *it == format!("Edit({glob})"))
            && settings
                .deny
                .iter()
                .any(|it| *it == format!("Edit({glob}/**)"))
    };
    // Every name the class holds with what is below it, for the commands Claude Code's sandbox
    // runs and for its own Edit and Write tools. A name that holds only itself (`.git`) is the
    // stated gap #1065: no glob here denies it without denying what git writes below it.
    for planted in PLANTED {
        let glob = format!("**/{}", planted.path);
        match planted.reach {
            Reach::AndBelow => assert!(held(&glob), "{glob} is not held: {settings:?}"),
            Reach::Itself => assert!(!deny_write.contains(&glob), "{glob}"),
        }
    }
    // Each harness's project config that can start code or change a server's command or
    // environment at its next launch (#1057), written out.
    for glob in [
        "**/.mcp.json",
        "**/.claude/settings.json",
        "**/.claude/settings.local.json",
        "**/.claude/commands",
        "**/.claude/agents",
        "**/.claude/skills",
        "**/opencode.json",
        "**/opencode.jsonc",
        "**/.opencode",
        "**/tui.json",
        "**/tui.jsonc",
        "**/.codex",
        "**/.agents",
    ] {
        assert!(held(glob), "{glob} is not held");
    }
}

#[test]
fn on_macos_the_store_holds_the_credential_store_so_a_claude_code_chat_compiles() {
    // Ruling V90a: charter writes every keyring item so that only its own app reads it without
    // the person's confirmation, so the vaults class is held by the store for every harness.
    let denied = Denied {
        paths: vec![],
        services: vec![Service::CredentialStore],
        unread: None,
    };
    claude::settings(&compiled(denied, Os::MacOs)).expect("compiles on macOS");
}

#[test]
fn on_linux_claude_code_cannot_hold_the_credential_store_so_the_chat_does_not_start() {
    // The Secret Service keeps no per-program rule, and Claude Code's own sandbox has not been
    // measured keeping a command off the session bus (ruling V90c).
    let denied = Denied {
        paths: vec![],
        services: vec![Service::CredentialStore],
        unread: None,
    };
    let refused = claude::settings(&compiled(denied, Os::Linux)).expect_err("refused");
    assert_eq!(refused.class(), Some(Class::Vaults));
}

// -------------------------------------------------------------------------------------
// Codex: what a chat's own words may not say to the Codex charter wraps
// -------------------------------------------------------------------------------------

fn words(line: &str) -> Vec<String> {
    line.split(' ').map(str::to_owned).collect()
}

#[test]
fn a_codex_command_that_would_drop_or_widen_the_sandbox_is_named() {
    // Each measured on codex-cli 0.147.0 or read from its source: `-s` of any value and the
    // bypass drop the profile whole, `--add-dir` and `--cd` move what is writable, `-a` and
    // `--approve-for-me` outrank the approval policy, `--search` turns the web search back on,
    // and a feature flag's effect on the sandbox is unmeasured, one feature at a time.
    for (line, flag) in [
        ("codex -s danger-full-access", "-s"),
        ("codex -s read-only", "-s"),
        ("codex --sandbox=workspace-write", "--sandbox"),
        ("codex -sdanger-full-access", "-s"),
        (
            "codex --dangerously-bypass-approvals-and-sandbox",
            "--dangerously-bypass-approvals-and-sandbox",
        ),
        ("codex --yolo", "--yolo"),
        ("codex --add-dir /home/op", "--add-dir"),
        ("codex -C /home/op", "-C"),
        ("codex --cd=/home/op", "--cd"),
        ("codex -a on-request", "-a"),
        ("codex --ask-for-approval untrusted", "--ask-for-approval"),
        ("codex --approve-for-me", "--approve-for-me"),
        ("codex --not-so-yolo", "--not-so-yolo"),
        ("codex --search", "--search"),
        ("codex --disable network_proxy", "--disable"),
        ("codex --disable=network_proxy", "--disable"),
        ("codex --enable browser_use", "--enable"),
        ("codex --enable=respect_system_proxy", "--enable"),
    ] {
        let why = codex::loosened_by(&words(line)).unwrap_or_else(|| panic!("{line}"));
        assert!(why.contains(&format!("`{flag}`")), "{line}: {why}");
    }
}

#[test]
fn a_codex_config_override_outside_the_few_that_cannot_touch_the_sandbox_is_named() {
    // Each class a `-c` could reach the sandbox through, in each spelling Codex takes.
    for key in [
        "sandbox_mode=\"danger-full-access\"",
        "sandbox_workspace_write.network_access=true",
        "profile=\"loose\"",
        "default_permissions=\":danger-full-access\"",
        "permissions.mine.network.enabled=true",
        "features.network_proxy=false",
        "tools.web_search=true",
        "approval_policy=\"on-request\"",
        "approvals_reviewer=\"auto_review\"",
        "web_search=\"live\"",
        "network.enabled=true",
        "hooks.Stop=[]",
        "mcp_servers.x.command=\"sh\"",
        "shell_environment_policy.inherit=\"all\"",
    ] {
        let name = key.split('=').next().expect("a key");
        for line in [
            format!("codex -c {key}"),
            format!("codex --config {key}"),
            format!("codex --config={key}"),
            format!("codex -c{key}"),
        ] {
            let why = codex::loosened_by(&words(&line)).unwrap_or_else(|| panic!("{line}"));
            assert!(why.contains(&format!("`-c {name}`")), "{line}: {why}");
        }
    }
}

#[test]
fn a_codex_command_that_only_tightens_or_does_not_touch_the_sandbox_is_not_named() {
    for line in [
        "codex",
        "codex -a never",
        "codex --ask-for-approval=never",
        // Measured: a `-p` profile's `sandbox_mode`, `default_permissions`,
        // `sandbox_workspace_write`, approval policy, web search and proxy feature all lose to
        // charter's flags.
        "codex -m gpt-5 -p work",
        "codex resume 0199",
        "codex -c model=\"o3\"",
        "codex --config model_reasoning_effort=\"high\"",
        "codex -c model_providers.local.base_url=\"http://localhost:1234/v1\"",
    ] {
        assert_eq!(codex::loosened_by(&words(line)), None, "{line}");
    }
    // A first message is not a flag, even one that starts with a dash.
    assert_eq!(codex::loosened_by(&["-a quick fix".to_owned()]), None);
}

/// The arguments `applied` gives a chat with these words, or why it may not start.
fn args_of(
    applied: &Applied,
    command: Vec<String>,
    armed: Vec<String>,
    charters: Vec<String>,
) -> Result<Vec<String>, String> {
    applied
        .line(
            Words {
                program: "codex".to_owned(),
                command,
                armed,
                charters,
            },
            &At {
                cwd: Some(applied.root()),
                ..At::default()
            },
        )
        .map(|line| line.args)
}

/// What a Codex chat starts under in `plane`, wrapped (#1123).
fn codex_sandbox_in(plane: &tempfile::TempDir) -> Applied {
    for_start(Harness::Codex, plane.path(), &machine(Os::MacOs), &|_| true)
        .expect("starts")
        .expect("sandboxed")
}

#[test]
fn a_sandbox_is_refused_a_line_that_would_drop_it_and_says_where_the_flag_came_from() {
    let plane = plane_saying(ON);
    let applied = codex_sandbox_in(&plane);
    assert_eq!(
        args_of(
            &applied,
            words("-s danger-full-access"),
            Vec::new(),
            Vec::new()
        ),
        Err(
            "this project runs every chat sandboxed, and the profile's command names `-s`, which \
             would run Codex outside the sandbox purlis compiled for it, so nothing was \
             started. Take it out of the profile's command."
                .to_owned()
        )
    );
    assert_eq!(
        args_of(
            &applied,
            Vec::new(),
            Vec::new(),
            words("-c sandbox_mode=\"danger-full-access\"")
        ),
        Err(
            "this project runs every chat sandboxed, and the chat's own arguments name \
             `-c sandbox_mode`, which would run Codex outside the sandbox purlis compiled for \
             it, so nothing was started. Start it without that argument."
                .to_owned()
        )
    );
}

// -------------------------------------------------------------------------------------
// The start: sandboxed, or not started (fail closed)
// -------------------------------------------------------------------------------------

fn plane_saying(toml: &str) -> tempfile::TempDir {
    let plane = tempfile::tempdir().expect("a plane");
    std::fs::write(plane.path().join("charter.toml"), toml).expect("charter.toml");
    plane
}

fn machine(os: Os) -> Machine {
    Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(std::path::PathBuf::from("/home/op")),
        os,
    }
}

const ON: &str = "[sandbox]\nmode = \"on\"\n";

/// [`ON`] without the `toolchains` preset, for a test whose machine has no data home to make the
/// project's package caches in (#1337): a start there is refused, naming the folder.
const ON_WITHOUT_CACHES: &str =
    "[sandbox]\nmode = \"on\"\negress = [\"model-providers\", \"forge\"]\n";

#[test]
fn a_chat_in_a_plane_that_says_nothing_starts_as_it_always_has() {
    let plane = plane_saying("schema = 1\n");
    let started = for_start(Harness::Codex, plane.path(), &machine(Os::Windows), &|_| {
        false
    });
    assert_eq!(started, Ok(None));
}

#[test]
fn a_claude_code_chat_in_a_sandboxed_plane_starts_sandboxed_for_claude_code() {
    let plane = plane_saying(ON);
    let applied = for_start(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Linux),
        &|_| true,
    )
    .expect("starts")
    .expect("sandboxed");
    assert_eq!(applied.harness(), Harness::ClaudeCode);
    let Form::ClaudeCode(settings) = applied.form() else {
        panic!("compiled for Claude Code: {:?}", applied.form());
    };
    assert_eq!(settings.sandbox["enabled"], true);
}

#[test]
fn a_mistyped_mode_does_not_start_a_chat_unsandboxed() {
    let plane = plane_saying("[sandbox]\nmode = \"of\"\n");
    let refused = for_start(
        Harness::Opencode,
        plane.path(),
        &machine(Os::Linux),
        &|_| true,
    );
    assert_eq!(
        refused,
        Err(NotStarted::Uncompilable(Uncompilable {
            harness: Harness::Opencode,
            unheld: Unheld::Wrap(Os::Linux),
        }))
    );
}

#[test]
fn a_linux_machine_without_socat_is_told_which_program_and_how_to_install_it() {
    let plane = plane_saying(ON);
    let refused = for_start(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Linux),
        &|program| program == "bwrap",
    )
    .expect_err("refused");
    assert_eq!(
        refused.to_string(),
        "this project runs every chat sandboxed, and this machine cannot apply the sandbox: socat \
         is not installed — install it with `sudo apt install socat` or `sudo dnf install \
         socat`. Nothing was started."
    );
}

#[test]
fn bwrap_is_installed_as_the_bubblewrap_package() {
    let missing = backend::missing(Os::Linux, &|_| false).expect("missing");
    assert_eq!(
        missing.to_string(),
        "bwrap and socat are not installed — install them with `sudo apt install bubblewrap socat` \
         or `sudo dnf install bubblewrap socat`"
    );
}

#[test]
fn on_windows_a_sandboxed_plane_starts_no_chat_until_a_backend_exists() {
    let plane = plane_saying(ON);
    let refused = for_start(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Windows),
        &|_| true,
    );
    assert_eq!(
        refused,
        Err(NotStarted::NoBackend(backend::Missing::NoBackend(
            Os::Windows
        )))
    );
}

fn plane_with_a_keyring_vault() -> tempfile::TempDir {
    let plane = plane_saying(ON);
    std::fs::write(
        plane.path().join("vaults.json"),
        r#"{"vaults": {"dev": {"provider": "keyring"}}}"#,
    )
    .expect("the registry");
    plane
}

#[test]
fn a_sandboxed_project_with_a_keyring_vault_starts_every_harness_on_macos() {
    // Ruling V90: the store holds Claude Code's, and charter's own wrap Codex's and opencode's.
    let plane = plane_with_a_keyring_vault();
    for harness in [Harness::ClaudeCode, Harness::Codex, Harness::Opencode] {
        let applied = for_start(harness, plane.path(), &machine(Os::MacOs), &|_| true)
            .unwrap_or_else(|refused| panic!("{harness:?}: {refused}"));
        assert!(applied.is_some(), "{harness:?} starts sandboxed");
    }
}

#[test]
fn on_linux_a_keyring_vault_refusal_offers_the_opt_out_and_moving_the_vault() {
    let plane = plane_with_a_keyring_vault();
    let refused = for_start(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Linux),
        &|_| true,
    )
    .expect_err("refused");
    let said = refused.to_string();
    assert_eq!(
        said,
        "this project runs every chat sandboxed, and on Linux purlis cannot keep a Claude \
         Code chat away from the system keyring, where this project's keyring vaults keep their \
         secrets, so nothing was started. Start this chat without the sandbox from the \
         new-chat picker, or move those secrets to a plain-file or 1Password vault, which the \
         sandbox can keep from a chat. For a resumed or relaunched chat, moving them is the \
         way on."
    );
    assert!(!said.contains("cannot keep a chat away from the operating system's credential store"));
}

#[test]
fn claude_code_is_denied_submodule_git_config_and_hook_managers_at_any_depth() {
    // Measured on 2.1.288: a `**` in the middle of a `denyWrite` glob holds at any depth.
    let settings = claude::settings(&compiled(Denied::default(), Os::MacOs)).expect("compiles");
    let write = &settings.sandbox["filesystem"]["denyWrite"];
    for glob in [
        "**/.git/modules/**/config",
        "**/.git/modules/**/hooks",
        "**/.husky",
        "**/.githooks",
    ] {
        assert!(
            write
                .as_array()
                .expect("a list")
                .iter()
                .any(|it| it == glob),
            "{glob} in {write}"
        );
        assert!(settings.deny.contains(&format!("Edit({glob})")), "{glob}");
    }
}

#[test]
fn a_clone_the_app_made_for_a_chat_keeps_its_git_config_hooks_and_editor_settings_denied_to_it() {
    // #1335: the app writes a sandboxed chat's clone and worktree for it, and the chat's own
    // writes to what they carry stay refused — `git config` included, at any depth.
    let settings = claude::settings(&compiled(Denied::default(), Os::MacOs)).expect("compiles");
    let write = settings.sandbox["filesystem"]["denyWrite"]
        .as_array()
        .expect("a list")
        .clone();
    for glob in [
        "**/.git/config",
        "**/.git/hooks",
        "**/.git/worktrees",
        "**/.vscode",
        "**/.claude/settings.json",
        // What would point git at a config, objects or attributes of the chat's own (D-1335-7).
        "**/.git/commondir",
        "**/.git/objects/info/alternates",
        "**/.git/info/attributes",
        "**/.git/modules/**/commondir",
        "**/.git/modules/**/objects/info/alternates",
        "**/.git/modules/**/info/attributes",
    ] {
        assert!(write.iter().any(|it| it == glob), "{glob} in {write:?}");
        assert!(settings.deny.contains(&format!("Edit({glob})")), "{glob}");
    }
}

#[test]
fn a_chat_in_a_branch_folder_is_given_nothing_under_its_clones_git_directory() {
    // #1055, ruling V99h: a sandboxed chat in a linked worktree commits through the app, and
    // its sandbox is not widened for it. The worktree's git directory is in its clone's
    // `.git/worktrees`, which stays a later-code name on every harness, and the clone is
    // outside what a chat standing in the branch folder writes.
    for name in [
        ".git",
        ".git/worktrees",
        ".git/commondir",
        ".git/config.worktree",
    ] {
        assert!(PLANTED.iter().any(|planted| planted.path == name), "{name}");
    }
    let cwd = Path::new("/project/workspaces/w/.worktrees/repo/piece");
    let tmp = Path::new("/tmp-of/the-chat");
    let profile =
        seatbelt::profile(&[], &seatbelt::Own::default(), cwd, tmp, 4040, None).expect("a profile");
    // The wrap (Codex, opencode): the two folders it may write, and no other folder of the
    // project, the clone least of all.
    let subpaths: Vec<&str> = profile
        .lines()
        .skip_while(|line| *line != "(allow file-write*")
        .skip(1)
        .take_while(|line| line.trim_start().starts_with('('))
        .filter(|line| line.contains("subpath"))
        .collect();
    assert_eq!(
        subpaths,
        [
            "  (subpath \"/project/workspaces/w/.worktrees/repo/piece\")",
            "  (subpath \"/tmp-of/the-chat\")",
        ],
        "{profile}"
    );
    assert!(!profile.contains("/project/workspaces/w/repo"), "{profile}");
    // And the folder's own `.git` file is held there, so the link cannot be rewritten.
    let rules = seatbelt::planted_rules(cwd).expect("rules");
    assert!(
        rules.iter().any(|rule| rule.contains("\\\\.git$")),
        "{rules:?}"
    );
    // Claude Code: no write is added, and the worktree's git directory stays denied by name.
    let settings = claude::settings(&compiled(Denied::default(), Os::MacOs)).expect("compiles");
    let filesystem = &settings.sandbox["filesystem"];
    assert!(
        deny_write(&settings)
            .iter()
            .any(|it| it == "**/.git/worktrees"),
        "{filesystem}"
    );
    assert!(
        !filesystem.to_string().contains("allowWrite")
            || !filesystem["allowWrite"].to_string().contains(".git"),
        "{filesystem}"
    );
}

#[test]
fn what_a_hooks_path_or_a_project_config_names_is_denied_to_every_harness_from_the_start() {
    // Ruling V73d: resolved when the chat starts, as path denials of the later-code class.
    let plane = plane_saying(ON);
    let clone = plane.path().join("workspaces/w/repo");
    std::fs::create_dir_all(clone.join(".git")).expect("a clone");
    std::fs::write(clone.join(".git/config"), "[core]\nhooksPath = hooks\n").expect("config");
    std::fs::create_dir_all(clone.join(".claude")).expect(".claude");
    std::fs::write(
        clone.join(".claude/settings.local.json"),
        r#"{"hooks": {"PreToolUse": [{"hooks": [{"command": "./guard.sh"}]}]}}"#,
    )
    .expect("settings");
    let denied = Denied::of(plane.path(), &machine(Os::MacOs));
    for want in [clone.join("hooks"), clone.join("guard.sh")] {
        assert!(
            denied.paths.iter().any(|it| it.path == want
                && it.class == Class::LaterCode
                && it.access == Access::Write),
            "{want:?} not denied: {:?}",
            denied.paths
        );
    }
}

/// Whether `denied` holds `path` as later code a chat may not write.
fn denies_later_code(denied: &Denied, path: &std::path::Path) -> bool {
    denied
        .paths
        .iter()
        .any(|it| it.path == path && it.class == Class::LaterCode && it.access == Access::Write)
}

/// Gives the project at `plane` its own hook that runs `command`.
fn hook_runs(plane: &std::path::Path, command: &str) {
    std::fs::create_dir_all(plane.join(".claude")).expect(".claude");
    let settings = serde_json::json!({"hooks": {"PreToolUse": [{"hooks": [
        {"type": "command", "command": command}
    ]}]}});
    std::fs::write(
        plane.join(".claude/settings.local.json"),
        settings.to_string(),
    )
    .expect("settings");
}

#[test]
fn a_later_letter_s_value_in_a_folder_chats_may_be_granted_is_denied() {
    // D-T56-1: a folder you list as one chats may be granted (D-1342-10), outside the project
    // and the home folder, is one a chat could be given to write; `-xI<there>` names it.
    let plane = tempfile::tempdir().expect("a plane");
    hook_runs(plane.path(), "cc -xI/opt/granted/inc/x.h a.c");
    let want = std::path::Path::new("/opt/granted/inc/x.h");
    assert!(!denies_later_code(
        &Denied::of(plane.path(), &machine(Os::Linux)),
        want
    ));
    local::list_grantable(plane.path(), std::path::Path::new("/opt/granted")).expect("listed");
    let denied = Denied::of(plane.path(), &machine(Os::Linux));
    assert!(denies_later_code(&denied, want), "{:?}", denied.paths);
}

#[test]
fn a_later_letter_s_value_in_the_project_s_cache_home_is_denied() {
    // D-T56-1: the project's cache home (#1337) is under purlis's data home, which
    // `XDG_DATA_HOME` can move out of the home folder; a chat writes it.
    let machine = Machine {
        env: crate::secrets::Env::of(&[("XDG_DATA_HOME", "/srv/data")]),
        home: Some(std::path::PathBuf::from("/home/op")),
        os: Os::Linux,
    };
    let plane = tempfile::tempdir().expect("a plane");
    let cache = caches::root_of(&machine, plane.path()).expect("a data home");
    assert!(cache.starts_with("/srv/data"), "{cache:?}");
    // A cluster whose letters purlis does not know (gawk's own are read as gawk reads them,
    // D-1418-1).
    let want = cache.join("npm/x.h");
    hook_runs(plane.path(), &format!("cc -xI{} a.c", want.display()));
    assert!(!denies_later_code(
        &Denied::of(
            plane.path(),
            &Machine {
                env: crate::secrets::Env::of(&[]),
                ..machine.clone()
            }
        ),
        &want
    ));
    let denied = Denied::of(plane.path(), &machine);
    assert!(denies_later_code(&denied, &want), "{:?}", denied.paths);
}

#[test]
fn a_charter_toml_that_cannot_be_read_starts_no_chat_rather_than_one_unsandboxed() {
    // Absent `[sandbox]` is "not set", so a file charter cannot parse must not read as that.
    let plane = plane_saying("[sandbox\nmode = \"on\"\n");
    for harness in Harness::ALL {
        let refused =
            for_start(harness, plane.path(), &machine(Os::MacOs), &|_| true).expect_err("refused");
        assert_eq!(refused, NotStarted::PlaneUnreadable, "{harness:?}");
        assert_eq!(
            refused.to_string(),
            "charter.toml in this plane cannot be read as TOML, so purlis cannot tell whether \
             it runs chats sandboxed, and nothing was started. Fix charter.toml and start the \
             chat again."
        );
    }
    // No file at all says nothing, as before.
    let none = tempfile::tempdir().expect("a directory");
    assert_eq!(
        for_start(Harness::Codex, none.path(), &machine(Os::MacOs), &|_| true),
        Ok(None)
    );
}

#[test]
fn the_directories_between_a_root_and_a_path_are_each_named_once() {
    let root = std::path::Path::new("/p");
    assert_eq!(
        ancestors_within(std::path::Path::new("/p/.charter/app/spool"), root),
        [
            std::path::PathBuf::from("/p/.charter"),
            std::path::PathBuf::from("/p/.charter/app")
        ]
    );
    assert!(ancestors_within(std::path::Path::new("/p/x"), root).is_empty());
    assert!(ancestors_within(std::path::Path::new("/q/x/y"), root).is_empty());
}

/// What `for_start` answers in a plane whose `charter.toml` is `make`'s.
#[cfg(unix)]
fn started_with(make: impl Fn(&std::path::Path)) -> Result<Option<Applied>, NotStarted> {
    let plane = tempfile::tempdir().expect("a plane");
    make(&plane.path().join("charter.toml"));
    for_start(Harness::Codex, plane.path(), &machine(Os::MacOs), &|_| true)
}

/// What puts something at a path, for a test of what is there.
#[cfg(unix)]
type Maker = dyn Fn(&std::path::Path) + Send;

#[cfg(unix)]
#[test]
fn a_charter_toml_that_is_not_a_regular_file_starts_no_chat() {
    let sandboxed = |dir: &std::path::Path| {
        let target = dir.join("real.toml");
        std::fs::write(&target, ON).expect("a real file");
        target
    };
    // A link to a file that turns the sandbox on, a dangling link, a directory, a FIFO, a socket.
    let cases: Vec<(&str, Box<Maker>)> = vec![
        (
            "a link",
            Box::new(move |at: &std::path::Path| {
                let target = sandboxed(at.parent().expect("a parent"));
                std::os::unix::fs::symlink(target, at).expect("a link");
            }),
        ),
        (
            "a dangling link",
            Box::new(|at: &std::path::Path| {
                std::os::unix::fs::symlink(at.with_extension("gone"), at).expect("a link");
            }),
        ),
        (
            "a directory",
            Box::new(|at: &std::path::Path| std::fs::create_dir(at).expect("a directory")),
        ),
        (
            "a FIFO",
            Box::new(|at: &std::path::Path| {
                let made = crate::forklock::status(std::process::Command::new("mkfifo").arg(at))
                    .expect("mkfifo runs");
                assert!(made.success(), "a FIFO");
            }),
        ),
        (
            "a socket",
            Box::new(|at: &std::path::Path| {
                std::mem::forget(std::os::unix::net::UnixListener::bind(at).expect("a socket"));
            }),
        ),
    ];
    for (what, make) in cases {
        let (sender, answer) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(started_with(make).map(|it| it.is_some()));
        });
        // A FIFO with no writer must not hold the start up.
        let started = answer
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap_or_else(|_| panic!("{what}: the start blocked"));
        assert_eq!(started, Err(NotStarted::PlaneUnreadable), "{what}");
    }
}

#[cfg(unix)]
#[test]
fn a_device_is_not_read_as_a_plane_file() {
    // An endless device would otherwise be read until memory ran out.
    for device in ["/dev/zero", "/dev/null"] {
        assert_eq!(
            read_plane_file(std::path::Path::new(device)),
            Err(()),
            "{device}"
        );
    }
}

#[test]
fn a_charter_toml_over_the_cap_starts_no_chat() {
    let plane = tempfile::tempdir().expect("a plane");
    let mut text = String::from(ON);
    text.push_str(&"# padding\n".repeat(PLANE_FILE_MAX / 10 + 1));
    std::fs::write(plane.path().join("charter.toml"), &text).expect("charter.toml");
    assert_eq!(
        for_start(Harness::Codex, plane.path(), &machine(Os::MacOs), &|_| true),
        Err(NotStarted::PlaneUnreadable)
    );
    // Under the cap, the same words are read.
    std::fs::write(plane.path().join("charter.toml"), ON).expect("charter.toml");
    assert!(
        for_start(
            Harness::ClaudeCode,
            plane.path(),
            &machine(Os::MacOs),
            &|_| true
        )
        .expect("starts")
        .is_some()
    );
}

#[cfg(unix)]
#[test]
fn no_sandboxed_chat_starts_in_a_folder_reached_through_a_link() {
    // Ruling of 2026-10-03, every harness: a plane-root chat could swap a workspace folder for
    // a link, and a chat started there would take that folder's rules to the link's target.
    let plane = plane_saying(ON_WITHOUT_CACHES);
    let workspaces = plane.path().join("workspaces");
    std::fs::create_dir_all(workspaces.join("real")).expect("a real workspace");
    let elsewhere = tempfile::tempdir().expect("somewhere outside");
    std::os::unix::fs::symlink(elsewhere.path(), workspaces.join("swapped")).expect("a link");
    std::fs::write(workspaces.join("a-file"), "").expect("a file");
    std::os::unix::fs::symlink(workspaces.join("real"), plane.path().join("ws-link"))
        .expect("a link to a real folder inside");
    let words = |program: &str| Words {
        program: program.to_owned(),
        command: Vec::new(),
        armed: Vec::new(),
        charters: Vec::new(),
    };
    for harness in Harness::ALL {
        let applied =
            compiled_anyway(harness, plane.path(), &machine(Os::MacOs)).expect("compiles");
        let confinement = applied.confine().expect("confined");
        let at = |cwd: std::path::PathBuf| (cwd, confinement.as_ref());
        for (cwd, want) in [
            (workspaces.join("swapped"), FolderRefusal::Linked),
            (workspaces.join("swapped/below"), FolderRefusal::Linked),
            (plane.path().join("ws-link"), FolderRefusal::Linked),
            (workspaces.join("a-file"), FolderRefusal::Linked),
            (workspaces.join("missing"), FolderRefusal::Linked),
            (elsewhere.path().to_path_buf(), FolderRefusal::Outside),
        ] {
            let (cwd, confinement) = at(cwd);
            assert_eq!(
                applied.line(
                    words("harness"),
                    &At {
                        cwd: Some(&cwd),
                        confinement,
                        ..At::default()
                    }
                ),
                Err(want.said(&policy::Locks::none())),
                "{harness:?} in {}",
                cwd.display()
            );
        }
        // A real folder inside the plane starts.
        assert!(
            applied
                .line(
                    words("harness"),
                    &At {
                        cwd: Some(&workspaces.join("real")),
                        confinement: confinement.as_ref(),
                        ..At::default()
                    }
                )
                .is_ok(),
            "{harness:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_charter_toml_above_a_folder_that_is_not_a_regular_file_is_seen() {
    // The plane's own walk takes only a regular file for a plane, so a start that asked it
    // alone would read "no plane" and start the chat unsandboxed. Under either name (RN-2a).
    for (name, kind) in ["charter.toml", "purlis.toml"]
        .into_iter()
        .flat_map(|name| ["link", "dangling", "directory", "fifo"].map(|kind| (name, kind)))
    {
        let plane = tempfile::tempdir().expect("a plane");
        let marker = plane.path().join(name);
        match kind {
            "link" => {
                std::fs::write(plane.path().join("real.toml"), ON).expect("a file");
                std::os::unix::fs::symlink(plane.path().join("real.toml"), &marker)
                    .expect("a link");
            }
            "dangling" => std::os::unix::fs::symlink("gone", &marker).expect("a link"),
            "directory" => std::fs::create_dir(&marker).expect("a directory"),
            _ => {
                let made =
                    crate::forklock::status(std::process::Command::new("mkfifo").arg(&marker))
                        .expect("mkfifo runs");
                assert!(made.success());
            }
        }
        let below = plane.path().join("workspaces/w");
        std::fs::create_dir_all(&below).expect("a workspace");
        assert!(marker_unreadable(&below), "{name}: {kind}");
    }
    let plain = plane_saying(ON);
    assert!(!marker_unreadable(plain.path()));
    let none = tempfile::tempdir().expect("no plane");
    assert!(!marker_unreadable(none.path()));
}

#[cfg(unix)]
#[test]
fn a_folder_is_found_inside_its_plane_whichever_side_names_it_through_a_link() {
    // The no-profile path takes the plane as the kernel names it and the folder as the chat
    // recorded it (`/tmp/…` against `/private/tmp/…` on macOS, or a linked parent anywhere).
    let base = tempfile::tempdir().expect("a base");
    let real_parent = base.path().join("real");
    std::fs::create_dir_all(real_parent.join("plane/workspaces/w")).expect("a plane");
    std::fs::write(real_parent.join("plane/charter.toml"), ON).expect("charter.toml");
    std::os::unix::fs::symlink(&real_parent, base.path().join("linked")).expect("a linked parent");
    let real = real_parent.join("plane").canonicalize().expect("real");
    let linked = base.path().join("linked/plane");
    for (root, cwd) in [
        (real.clone(), linked.join("workspaces/w")),
        (linked.clone(), real.join("workspaces/w")),
        (linked.clone(), linked.join("workspaces/w")),
        (real.clone(), real.join("workspaces/w")),
    ] {
        assert_eq!(
            folder_refusal(&root, &cwd),
            None,
            "{} in {}",
            cwd.display(),
            root.display()
        );
    }
    // A link inside the plane is still one, from either side.
    std::os::unix::fs::symlink(real.join("workspaces/w"), real.join("workspaces/again"))
        .expect("a link inside");
    for (root, cwd) in [
        (real.clone(), linked.join("workspaces/again")),
        (linked.clone(), real.join("workspaces/again")),
    ] {
        assert_eq!(
            folder_refusal(&root, &cwd),
            Some(FolderRefusal::Linked),
            "{}",
            cwd.display()
        );
    }
}

#[cfg(unix)]
#[test]
fn every_harness_is_compiled_against_the_plane_as_the_kernel_names_it() {
    // A plane reached through a link (`/tmp` on macOS, a linked parent anywhere): one spelling
    // for every harness, so no rule depends on how the caller wrote the root.
    let base = tempfile::tempdir().expect("a base");
    std::fs::create_dir_all(base.path().join("real/plane")).expect("a plane");
    std::fs::write(base.path().join("real/plane/charter.toml"), ON).expect("charter.toml");
    std::os::unix::fs::symlink(base.path().join("real"), base.path().join("tmp"))
        .expect("a linked parent");
    let linked = base.path().join("tmp/plane");
    let real = linked.canonicalize().expect("real");
    assert_ne!(linked, real);
    let applied = for_start(Harness::ClaudeCode, &linked, &machine(Os::MacOs), &|_| true)
        .expect("starts")
        .expect("sandboxed");
    assert_eq!(applied.root(), real.as_path());
    let Form::ClaudeCode(settings) = applied.form() else {
        panic!("compiled for Claude Code");
    };
    let write = settings.sandbox["filesystem"]["denyWrite"].to_string();
    assert!(
        write.contains(&real.join(".charter/app").display().to_string()),
        "{write}"
    );
    assert!(!write.contains(&linked.display().to_string()), "{write}");
}

/// The Claude Code settings a chat in `cwd` of the sandboxed plane at `root` is armed with.
fn claude_settings_in(root: &Path, cwd: &Path) -> claude::Settings {
    let applied = for_start(Harness::ClaudeCode, root, &machine(Os::MacOs), &|_| true)
        .expect("starts")
        .expect("sandboxed");
    let Form::ClaudeCode(settings) = applied.form_in(Some(cwd)) else {
        panic!("compiled for Claude Code");
    };
    settings
}

/// Whether `settings` deny writing `path`, to Claude Code's sandbox and to its Edit tool.
fn claude_holds(settings: &claude::Settings, path: &Path) -> bool {
    let path = path.display().to_string();
    deny_write(settings).contains(&path)
        && settings.deny.contains(&format!("Edit(/{path})"))
        && settings.deny.contains(&format!("Edit(/{path}/**)"))
}

#[test]
fn a_claude_code_chat_writes_a_manifest_only_where_it_cannot_change_a_chats_sandbox() {
    // #1336: a chat in `ws/repo` is denied the manifests of the root, `ws` and its own folder,
    // and may write one below its folder or in a temp folder.
    let plane = plane_saying(ON);
    let root = plane.path().canonicalize().expect("real");
    let repo = root.join("ws/repo");
    std::fs::create_dir_all(repo.join("sub")).expect("a repo");
    let settings = claude_settings_in(&root, &repo);
    for name in MANIFEST_NAMES {
        for folder in [root.clone(), root.join("ws"), repo.clone()] {
            let path = folder.join(name);
            assert!(
                claude_holds(&settings, &path),
                "{} is writable",
                path.display()
            );
        }
        let fixture = repo.join("sub").join(name);
        assert!(!claude_holds(&settings, &fixture), "{}", fixture.display());
        // No rule names a manifest by name alone, nor outside the project.
        let rules = deny_write(&settings)
            .into_iter()
            .chain(settings.deny.iter().cloned())
            .filter(|rule| rule.contains(name))
            .collect::<Vec<_>>();
        assert!(
            rules
                .iter()
                .all(|rule| !rule.contains("**/") && rule.contains(&root.display().to_string())),
            "{rules:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_claude_code_chat_reached_through_a_link_is_denied_its_manifests_by_both_names() {
    let base = tempfile::tempdir().expect("a base");
    std::fs::create_dir_all(base.path().join("real/plane/ws/repo")).expect("a plane");
    std::fs::write(base.path().join("real/plane/charter.toml"), ON).expect("charter.toml");
    std::os::unix::fs::symlink(base.path().join("real"), base.path().join("linked"))
        .expect("a linked parent");
    let linked = base.path().join("linked/plane");
    let real = linked.canonicalize().expect("real");
    assert_ne!(linked, real);
    let settings = claude_settings_in(&linked, &linked.join("ws/repo"));
    for name in MANIFEST_NAMES {
        for folder in ["", "ws", "ws/repo"] {
            for spelled in [&real, &linked] {
                let path = spelled.join(folder).join(name);
                assert!(
                    claude_holds(&settings, &path),
                    "{} is writable",
                    path.display()
                );
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn a_root_manifest_that_is_a_link_is_denied_to_claude_code_by_its_own_name_and_its_target() {
    // #1336: the link itself, so it is never removed and replaced, and what it points at.
    let plane = plane_saying(ON);
    let root = plane.path().canonicalize().expect("real");
    let dotfiles = tempfile::tempdir().expect("a dotfiles repository");
    let target = dotfiles.path().join("purlis.local.toml");
    std::fs::write(&target, "").expect("the operator's settings");
    std::os::unix::fs::symlink(&target, root.join("purlis.local.toml")).expect("a link");
    let settings = claude_settings_in(&root, &root);
    for path in [
        root.join("purlis.local.toml"),
        target.canonicalize().expect("real"),
    ] {
        assert!(
            claude_holds(&settings, &path),
            "{} is writable",
            path.display()
        );
    }
}

#[test]
fn a_manifest_planted_below_a_workspace_names_no_project_of_its_own() {
    // Why a manifest below the chat's folder is left writable: the walk re-roots a marker
    // inside a project's `workspaces/` to that project (`plane::outermost`).
    let plane = plane_saying(ON);
    let root = plane.path().canonicalize().expect("real");
    let sub = root.join("workspaces/w/sub");
    std::fs::create_dir_all(&sub).expect("a folder");
    std::fs::write(sub.join("purlis.toml"), "[sandbox]\n").expect("a planted manifest");
    assert_eq!(crate::plane::find_root(&sub).expect("a plane"), root);
}

#[cfg(unix)]
#[test]
fn a_manifest_of_the_chats_folders_linked_to_its_ground_refuses_the_chat() {
    // #1327's backstop over the manifests #1336 adds: a link a chat planted in its folder, to
    // that folder or to `/`, would leave a later chat there read-only, so it is refused.
    for target in ["sub", "/"] {
        let plane = plane_saying(ON);
        let root = plane.path().canonicalize().expect("real");
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).expect("a folder");
        let target = if target == "/" {
            std::path::PathBuf::from("/")
        } else {
            sub.clone()
        };
        std::os::unix::fs::symlink(&target, sub.join("purlis.local.toml")).expect("a link");
        let applied = for_start(Harness::ClaudeCode, &root, &machine(Os::MacOs), &|_| true)
            .expect("starts")
            .expect("sandboxed");
        let line = applied.line(
            Words {
                program: "claude".to_owned(),
                command: Vec::new(),
                armed: Vec::new(),
                charters: Vec::new(),
            },
            &At {
                cwd: Some(&sub),
                hook_socket: None,
                confinement: None,
                no_opt_out: false,
            },
        );
        let why = line.expect_err("refused");
        assert!(
            why.contains("purlis.local.toml") && why.contains("nothing was started"),
            "{}: {why}",
            target.display()
        );
    }
}

#[test]
fn whether_a_harness_ever_starts_sandboxed_here_is_its_compiler_its_hold_and_its_wrap() {
    assert_eq!(never_on(Harness::ClaudeCode, Os::MacOs), None);
    assert_eq!(never_on(Harness::Opencode, Os::MacOs), None);
    assert_eq!(
        never_on(Harness::Opencode, Os::Linux).as_deref(),
        Some("purlis can wrap it on macOS only, so far")
    );
    // #1123: charter wraps Codex as it wraps opencode.
    assert_eq!(never_on(Harness::Codex, Os::MacOs), None);
    assert_eq!(
        never_on(Harness::Codex, Os::Linux).as_deref(),
        Some("purlis can wrap it on macOS only, so far")
    );
}

/// #1422: the window shows why a held-back harness never starts sandboxed, and a ticket number
/// is no part of a sentence the window shows (`docs/ui-copy.md`). The issue stays with the
/// adapter that holds it back.
#[test]
fn a_held_back_harness_is_said_without_the_issue_it_waits_for() {
    let why = never_with(compiler(Harness::Codex), Some(1150), Os::MacOs)
        .expect("a held-back harness never starts sandboxed");
    assert_eq!(why, "purlis cannot keep its chats inside the sandbox yet");
    let refused = NotStarted::HeldBack(Harness::Codex, 1150).to_string();
    assert!(
        !refused.contains('#') && !refused.contains("1150"),
        "{refused}"
    );
}

/// #1422: which preset lets a chat write the project's package caches is the core's to say,
/// and the window reads it from the preset rather than keeping a copy of its word.
#[test]
fn only_the_toolchains_preset_widens_the_package_caches() {
    assert_eq!(
        Preset::ALL
            .into_iter()
            .filter(|preset| preset.widens_caches())
            .collect::<Vec<_>>(),
        [Preset::Toolchains]
    );
}

// -------------------------------------------------------------------------------------
// The per-chat opt-out (ADR 0067 §7) and Windows (ruling V21 3, V78 b)
// -------------------------------------------------------------------------------------

fn off(reason: Option<&str>) -> OptOut {
    OptOut {
        reason: reason.map(str::to_owned),
    }
}

#[test]
fn a_person_can_start_one_chat_without_the_sandbox_and_every_class_is_lifted_for_it() {
    let plane = plane_saying(ON);
    let decided = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::MacOs),
        &|_| true,
        Some(&off(Some("the build needs the network"))),
        None,
    );
    assert_eq!(
        decided,
        Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::Person,
            reason: Some("the build needs the network".to_owned()),
        })))
    );
    assert_eq!(Lifted::CLASSES, Class::ALL);
}

#[test]
fn a_project_whose_manifest_has_gone_never_reads_as_the_sandbox_being_off() {
    // D-1410e: a project with no manifest at its root cannot say whether it runs chats
    // sandboxed, so the chat is refused; a person's opt-out still starts it, audited.
    let gone = tempfile::tempdir().unwrap();
    for os in [Os::MacOs, Os::Linux] {
        let refused = decide(
            Harness::ClaudeCode,
            gone.path(),
            &machine(os),
            &|_| true,
            None,
            None,
        );
        assert_eq!(refused, Err(NotStarted::PlaneMissing), "{os:?}");
    }
    assert!(
        NotStarted::PlaneMissing
            .to_string()
            .starts_with(&format!("This project's {FILE} is missing")),
        "{}",
        NotStarted::PlaneMissing
    );
    let started = decide(
        Harness::ClaudeCode,
        gone.path(),
        &machine(Os::MacOs),
        &|_| true,
        Some(&off(None)),
        None,
    );
    assert_eq!(
        started,
        Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::Person,
            reason: None,
        })))
    );
}

#[test]
fn the_opt_out_is_what_lets_a_chat_start_where_the_sandbox_cannot_be_applied() {
    let plane = plane_saying(ON);
    let refused = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Linux),
        &|_| false,
        None,
        None,
    );
    assert!(
        matches!(refused, Err(NotStarted::NoBackend(_))),
        "{refused:?}"
    );
    let started = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Linux),
        &|_| false,
        Some(&off(None)),
        None,
    );
    assert_eq!(
        started,
        Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::Person,
            reason: None,
        })))
    );
}

#[test]
fn a_project_that_has_not_turned_the_sandbox_on_has_nothing_to_opt_out_of() {
    let plane = plane_saying("schema = 1\n");
    let decided = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::MacOs),
        &|_| true,
        Some(&off(Some("why not"))),
        None,
    );
    assert_eq!(decided, Ok(None), "no lift, so nothing to audit");
}

#[test]
fn without_an_opt_out_a_sandboxed_project_still_sandboxes_or_refuses() {
    let plane = plane_saying(ON);
    let decided = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::MacOs),
        &|_| true,
        None,
        None,
    );
    assert!(
        matches!(decided, Ok(Some(Decided::Sandboxed(ref applied))) if applied.harness() == Harness::ClaudeCode),
        "{decided:?}"
    );
}

#[test]
fn on_windows_a_chat_in_a_sandboxed_project_starts_at_the_opt_out_and_charter_is_the_actor() {
    let plane = plane_saying(ON);
    let decided = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Windows),
        &|_| true,
        None,
        None,
    );
    assert_eq!(
        decided,
        Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::NoBackend(Os::Windows),
            reason: None,
        })))
    );
}

#[test]
fn a_system_with_no_backend_that_is_not_windows_still_fails_closed() {
    let plane = plane_saying(ON);
    let decided = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Other),
        &|_| true,
        None,
        None,
    );
    assert_eq!(
        decided,
        Err(NotStarted::NoBackend(backend::Missing::NoBackend(
            Os::Other
        )))
    );
}

#[test]
fn a_typed_reason_is_kept_to_one_short_line() {
    let long = format!("first line\nsecond {}", "x".repeat(600));
    let plane = plane_saying(ON);
    let Ok(Some(Decided::Unsandboxed(lifted))) = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::MacOs),
        &|_| true,
        Some(&off(Some(&long))),
        None,
    ) else {
        panic!("unsandboxed");
    };
    let reason = lifted.reason.expect("a reason");
    assert!(!reason.contains('\n'), "{reason}");
    assert!(
        reason.chars().count() <= OptOut::MOST_REASON_CHARS,
        "{reason}"
    );
    let blank = decide(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::MacOs),
        &|_| true,
        Some(&off(Some("   "))),
        None,
    );
    assert_eq!(
        blank,
        Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::Person,
            reason: None,
        })))
    );
}

#[test]
fn an_unsandboxed_chat_says_so_on_its_tab_and_why() {
    assert_eq!(
        Lifted {
            by: By::Person,
            reason: None,
        }
        .notice(),
        "This chat runs without the sandbox: you turned it off for this chat only. A new or \
         resumed chat does not inherit it."
    );
    assert_eq!(
        Lifted {
            by: By::NoBackend(Os::Windows),
            reason: None,
        }
        .notice(),
        "This chat runs without the sandbox: purlis has no sandbox backend on Windows yet, so \
         every chat here starts without it until one exists."
    );
}

// -------------------------------------------------------------------------------------
// SD-30's install action (ruling V78 c): the distribution's own command, typed and never run
// -------------------------------------------------------------------------------------

#[test]
fn the_install_action_is_the_distributions_own_package_manager_command() {
    let missing = backend::missing(Os::Linux, &|_| false).expect("missing");
    let cases = [
        (
            "ID=ubuntu\nID_LIKE=debian\n",
            "sudo apt install bubblewrap socat",
        ),
        ("ID=debian\n", "sudo apt install bubblewrap socat"),
        ("ID=\"fedora\"\n", "sudo dnf install bubblewrap socat"),
        (
            "ID=\"rocky\"\nID_LIKE=\"rhel centos fedora\"\n",
            "sudo dnf install bubblewrap socat",
        ),
        ("ID=arch\n", "sudo pacman -S bubblewrap socat"),
        (
            "ID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n",
            "sudo zypper install bubblewrap socat",
        ),
        ("ID=alpine\n", "sudo apk add bubblewrap socat"),
    ];
    for (os_release, command) in cases {
        assert_eq!(
            backend::install_command(&missing, os_release).as_deref(),
            Some(command),
            "{os_release}"
        );
    }
}

#[test]
fn only_what_is_missing_is_installed() {
    let missing = backend::missing(Os::Linux, &|program| program == "bwrap").expect("missing");
    assert_eq!(
        backend::install_command(&missing, "ID=debian\n").as_deref(),
        Some("sudo apt install socat")
    );
}

#[test]
fn a_distribution_charter_does_not_know_gets_no_action_rather_than_a_guess() {
    let missing = backend::missing(Os::Linux, &|_| false).expect("missing");
    assert_eq!(backend::install_command(&missing, "ID=nixos\n"), None);
    assert_eq!(backend::install_command(&missing, ""), None);
}

#[test]
fn a_system_with_no_backend_has_nothing_to_install() {
    let missing = backend::missing(Os::Windows, &|_| false).expect("missing");
    assert_eq!(backend::install_command(&missing, "ID=debian\n"), None);
    let missing = backend::missing(Os::MacOs, &|_| false).expect("missing");
    assert_eq!(backend::install_command(&missing, "ID=debian\n"), None);
}

// -------------------------------------------------------------------------------------
// What the new-chat picker says before anything starts, and what each start records
// -------------------------------------------------------------------------------------

#[test]
fn the_picker_says_nothing_of_a_sandbox_a_project_has_not_turned_on() {
    let plane = plane_saying("schema = 1\n");
    assert_eq!(
        ahead(
            Harness::ClaudeCode,
            plane.path(),
            &machine(Os::Linux),
            &|_| false,
            "",
            &|_| Ok(()),
        ),
        Ahead::Off
    );
}

#[test]
fn the_picker_says_a_chat_will_be_sandboxed_where_it_can_be() {
    let plane = plane_saying(ON);
    assert_eq!(
        ahead(
            Harness::ClaudeCode,
            plane.path(),
            &machine(Os::MacOs),
            &|_| true,
            "",
            &|_| Ok(()),
        ),
        Ahead::Sandboxed
    );
}

#[test]
fn the_picker_shows_the_refusal_and_the_distributions_install_command_before_the_start() {
    let plane = plane_saying(ON);
    let Ahead::Refused { why, install } = ahead(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Linux),
        &|program| program == "bwrap",
        "ID=debian\n",
        &|_| Ok(()),
    ) else {
        panic!("refused");
    };
    assert!(why.contains("socat is not installed"), "{why}");
    assert_eq!(install.as_deref(), Some("sudo apt install socat"));
}

#[test]
fn a_refusal_nothing_can_be_installed_for_offers_no_install() {
    let plane = plane_saying(ON);
    std::fs::write(
        plane.path().join("vaults.json"),
        r#"{"vaults": {"dev": {"provider": "keyring"}}}"#,
    )
    .expect("the registry");
    let Ahead::Refused { install, .. } = ahead(
        Harness::ClaudeCode,
        plane.path(),
        &machine(Os::Linux),
        &|_| true,
        "ID=debian\n",
        &|_| Ok(()),
    ) else {
        panic!("refused");
    };
    assert_eq!(install, None);
}

#[test]
fn on_windows_the_picker_says_the_chat_starts_without_the_sandbox_and_why() {
    let plane = plane_saying(ON);
    assert_eq!(
        ahead(
            Harness::Codex,
            plane.path(),
            &machine(Os::Windows),
            &|_| true,
            "",
            &|_| Ok(()),
        ),
        Ahead::Unsandboxed(Lifted {
            by: By::NoBackend(Os::Windows),
            reason: None,
        })
    );
}

fn person() -> Lifted {
    Lifted {
        by: By::Person,
        reason: None,
    }
}

#[test]
fn a_new_chat_a_person_starts_unsandboxed_is_audited_off_and_counted_as_an_opt_out() {
    assert_eq!(
        at_start(Some(&person()), false, false, true),
        (Some(Change::Off(person())), Some(local::Started::OptedOut))
    );
}

#[test]
fn a_windows_start_is_audited_and_counted_apart_from_a_choice() {
    let windows = Lifted {
        by: By::NoBackend(Os::Windows),
        reason: None,
    };
    assert_eq!(
        at_start(Some(&windows), false, false, true),
        (
            Some(Change::Off(windows.clone())),
            Some(local::Started::NoBackend)
        )
    );
}

#[test]
fn a_chat_that_ran_unsandboxed_and_starts_sandboxed_again_is_audited_back_on_and_not_counted() {
    assert_eq!(at_start(None, true, true, false), (Some(Change::On), None));
}

#[test]
fn a_sandboxed_new_chat_is_counted_and_has_nothing_to_audit() {
    assert_eq!(
        at_start(None, true, false, true),
        (None, Some(local::Started::Sandboxed))
    );
}

#[test]
fn a_relaunch_is_audited_but_never_counted_again() {
    assert_eq!(
        at_start(Some(&person()), false, false, false),
        (Some(Change::Off(person())), None)
    );
}

#[test]
fn a_chat_in_a_project_without_the_sandbox_is_neither_audited_nor_counted() {
    assert_eq!(at_start(None, false, false, true), (None, None));
    assert_eq!(at_start(None, false, true, true), (None, None));
}

#[test]
fn an_unreadable_charter_toml_still_refuses_rather_than_reading_as_off() {
    let plane = plane_saying("[sandbox\nmode = \"on\"\n");
    assert_eq!(
        decide(
            Harness::ClaudeCode,
            plane.path(),
            &machine(Os::MacOs),
            &|_| true,
            None,
            None,
        ),
        Err(NotStarted::PlaneUnreadable)
    );
    assert!(
        matches!(
            ahead(
                Harness::ClaudeCode,
                plane.path(),
                &machine(Os::MacOs),
                &|_| true,
                "",
                &|_| Ok(()),
            ),
            Ahead::Refused { .. }
        ),
        "the picker shows the refusal"
    );
    // The opt-out sits inside that refusal, and is audited as every other one is.
    assert_eq!(
        decide(
            Harness::ClaudeCode,
            plane.path(),
            &machine(Os::MacOs),
            &|_| true,
            Some(&off(None)),
            None,
        ),
        Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::Person,
            reason: None,
        })))
    );
}

/// #1123 lifts ruling V87f's hold-back where charter wraps Codex. Where it cannot (Linux, #1040),
/// the picker shows that refusal before anything starts, and the opt-out inside it still starts
/// the chat, audited.
#[test]
fn the_picker_shows_codex_sandboxed_where_charter_wraps_it_and_the_opt_out_elsewhere() {
    let plane = plane_saying(ON);
    assert_eq!(
        ahead(
            Harness::Codex,
            plane.path(),
            &machine(Os::MacOs),
            &|_| true,
            "",
            &|_| Ok(()),
        ),
        Ahead::Sandboxed
    );
    let Ahead::Refused { why, install } = ahead(
        Harness::Codex,
        plane.path(),
        &machine(Os::Linux),
        &|_| true,
        "ID=debian\n",
        &|_| Ok(()),
    ) else {
        panic!("refused on Linux");
    };
    assert!(why.contains("not yet on Linux, so"), "{why}");
    assert!(!why.contains('#'), "{why}");
    assert_eq!(install, None, "nothing to install fixes it");
    assert_eq!(
        decide(
            Harness::Codex,
            plane.path(),
            &machine(Os::Linux),
            &|_| true,
            Some(&off(None)),
            None,
        ),
        Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::Person,
            reason: None,
        })))
    );
}

/// Ruling V87g, before the start: whatever the start's program check refuses — a relative
/// program, one wherever the chat can write, one not answering as the harness — is shown as a
/// refusal with "Start without the sandbox", and asked only where the chat would be sandboxed.
#[test]
fn the_picker_shows_every_refusal_of_the_program_check() {
    let plane = plane_saying(ON);
    for refusal in [
        NotStarted::ProgramRelative,
        NotStarted::ProgramWritable(plane.path().join("bin/claude")),
        NotStarted::WordWritable(plane.path().join("bin/run.sh").display().to_string()),
        NotStarted::WordTooLong,
        NotStarted::NotTheHarness(Harness::ClaudeCode),
    ] {
        let said = refusal.to_string();
        let check = |_: &Applied| Err(refusal.clone());
        assert_eq!(
            ahead(
                Harness::ClaudeCode,
                plane.path(),
                &machine(Os::MacOs),
                &|_| true,
                "",
                &check,
            ),
            Ahead::Refused {
                why: said,
                install: None,
            }
        );
    }
    let asked = std::sync::atomic::AtomicBool::new(false);
    let check = |_: &Applied| {
        asked.store(true, std::sync::atomic::Ordering::SeqCst);
        Err(NotStarted::ProgramRelative)
    };
    assert!(matches!(
        ahead(
            Harness::ClaudeCode,
            plane.path(),
            &machine(Os::Windows),
            &|_| true,
            "",
            &check
        ),
        Ahead::Unsandboxed(_)
    ));
    assert!(
        !asked.load(std::sync::atomic::Ordering::SeqCst),
        "not asked where the chat is not sandboxed"
    );
}

/// A file denial does not stop a connect to a unix socket: a sandbox treats that connect as
/// network. So no compiler may allow one beyond what it must, or a chat could reach
/// `charterd.sock` through a folder it cannot read (FD-27, ADR 0068 §5). What it must is the
/// chat's own hook socket, which a `purlis` command the chat runs asks the app over (ADR 0067
/// §2, #1328): compiled, Claude Code's sandbox allows no unix socket, and where the chat opens
/// it is allowed that one path, as the kernel names it, and never every socket. Codex and
/// opencode run in charter's wrap, which allows the hook socket alone (`codex_tests`,
/// `opencode_tests`).
#[test]
fn claude_code_allows_a_chat_no_unix_socket_but_its_hook_socket() {
    let (_plane, denied) = denied_with(None, Os::MacOs);
    let settings = claude::settings(&compiled(denied, Os::MacOs)).expect("compiles");
    let unix = |sandbox: &serde_json::Value| -> serde_json::Map<String, serde_json::Value> {
        sandbox["network"]
            .as_object()
            .expect("a network object")
            .iter()
            .filter(|(key, _)| key.to_lowercase().contains("unix"))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    };
    // Every socket refused in so many words, so a user's `true` cannot merge in.
    assert_eq!(
        serde_json::Value::Object(unix(&settings.sandbox)),
        serde_json::json!({"allowAllUnixSockets": false})
    );
    assert_eq!(settings.reporting_on(None), settings);

    let sockets = tempfile::tempdir().expect("a directory");
    let sockets_at = sockets.path().canonicalize().expect("the directory");
    let socket = sockets_at.join("hooks.sock");
    let reporting = settings.reporting_on(Some(&socket));
    assert_eq!(
        serde_json::Value::Object(unix(&reporting.sandbox)),
        serde_json::json!({
            "allowAllUnixSockets": false,
            "allowUnixSockets": [real(&socket).display().to_string()],
        })
    );
    // Nothing else moves but the socket's folder, denied to writes, so the bind the grant
    // carries can never replace the app's socket: as the kernel names it, and on the data
    // volume's other side where it has one (#1418).
    let folders: Vec<String> = std::iter::once(sockets_at.clone())
        .chain(firmlink_twin(&sockets_at))
        .map(|it| it.display().to_string())
        .collect();
    let mut without = reporting.clone();
    without.sandbox["network"]
        .as_object_mut()
        .expect("a network object")
        .remove("allowUnixSockets");
    let denied = without.sandbox["filesystem"]["denyWrite"]
        .as_array_mut()
        .expect("a denyWrite list");
    let added = denied.split_off(denied.len() - folders.len());
    assert_eq!(
        added,
        folders
            .iter()
            .map(|it| serde_json::json!(it))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        without.deny.split_off(settings.deny.len()),
        folders
            .iter()
            .flat_map(|folder| [format!("Edit(/{folder})"), format!("Edit(/{folder}/**)")])
            .collect::<Vec<_>>()
    );
    assert_eq!(without, settings);
}

#[test]
fn charters_own_wrap_never_lets_a_chat_ask_the_keychains_service() {
    // Ruling V90b: the wrap Codex and opencode run in denies the credential store's service.
    // Seatbelt denies by default, so it is held by no lookup of it being allowed, anywhere.
    let dir = tempfile::tempdir().expect("a directory");
    let cwd = dir.path().join("chat");
    let tmp = dir.path().join("tmp");
    let profile = seatbelt::profile(&[], &seatbelt::Own::default(), &cwd, &tmp, 4040, None)
        .expect("a profile");
    let probe = seatbelt::probe_profile(&tmp).expect("a profile");
    for text in [&profile, &probe] {
        assert!(text.starts_with("(version 1)\n(deny default)\n"), "{text}");
        for service in ["SecurityServer", "securityd", "com.apple.security"] {
            assert!(!text.contains(service), "{service} is reachable:\n{text}");
        }
        for line in text.lines().filter(|line| line.contains("mach-lookup")) {
            assert_eq!(line, "(allow mach-lookup", "a lookup not by name: {line}");
        }
    }
}

#[test]
fn a_chat_started_in_a_worktree_writes_that_folder_and_nothing_of_the_clone_it_was_cut_from() {
    // #1453: a dispatch may start its persona chat in a worktree the app cut for it. The start
    // is the one every chat in a branch folder has, and this is what it compiles to: the
    // worktree is the ground, and the clone it was cut from, that clone's `.git` and the
    // worktree's own git data there (`.git/worktrees/<piece>`) are not writable. That last one
    // is why such a chat's own `git commit` is refused and the app commits for it (#1055,
    // ruling V99h): the rule was kept, and a change to it fails here first.
    let dir = tempfile::tempdir().expect("a directory");
    let root = std::fs::canonicalize(dir.path()).expect("resolved");
    let clone = root.join("workspaces/alpha/api");
    let cwd = root.join("workspaces/alpha/.worktrees/api/check-the-queue-b5rc0def");
    let tmp = root.join("tmp");
    for made in [&clone, &cwd, &tmp] {
        std::fs::create_dir_all(made).expect("a folder");
    }

    let profile = seatbelt::profile(&[], &seatbelt::Own::default(), &cwd, &tmp, 4040, None)
        .expect("a profile");

    // Every path the profile lets the chat write, by the rule that names it.
    let block = profile
        .split_once("(allow file-write*\n")
        .expect("what it may write")
        .1;
    let allowed: Vec<&str> = block
        .lines()
        .map(str::trim)
        .take_while(|line| *line != ")")
        .filter(|line| line.starts_with("(subpath ") || line.starts_with("(literal "))
        .collect();
    let named = |path: &std::path::Path| format!("\"{}\"", path.display());
    assert!(
        allowed.contains(&format!("(subpath {})", named(&cwd)).as_str()),
        "{allowed:?}"
    );
    assert!(
        allowed.contains(&format!("(subpath {})", named(&tmp)).as_str()),
        "{allowed:?}"
    );
    // Nothing above the worktree, and nothing of the clone.
    for not_its in [
        clone.clone(),
        clone.join(".git"),
        clone.join(".git/worktrees/check-the-queue-b5rc0def"),
        root.join("workspaces/alpha"),
        root.join("workspaces/alpha/.worktrees"),
        root.join("workspaces/alpha/.worktrees/api"),
        root.clone(),
    ] {
        let rule = named(&not_its);
        assert!(
            !allowed.iter().any(|line| line.contains(rule.as_str())),
            "{} is writable: {allowed:?}",
            not_its.display()
        );
    }
    let writes: Vec<&&str> = allowed
        .iter()
        .filter(|line| line.contains(root.to_string_lossy().as_ref()))
        .collect();
    assert_eq!(
        writes.len(),
        2,
        "its folder and its temp folder: {writes:?}"
    );
}

// -------------------------------------------------------------------------------------
// The ground a chat stands on is never denied (#1327)
// -------------------------------------------------------------------------------------

fn later_code(path: &str, file: &str, word: &str) -> Denial {
    Denial {
        named: Some(Named {
            file: std::path::PathBuf::from(file),
            word: word.to_owned(),
        }),
        ..one(Class::LaterCode, path, Access::Write)
    }
}

/// `/`, the home directory, and the chat's folder with every folder above it.
fn ground_of(cwd: &str) -> Vec<std::path::PathBuf> {
    let mut ground: Vec<_> = std::path::Path::new(cwd)
        .ancestors()
        .map(std::path::Path::to_path_buf)
        .collect();
    ground.push(std::path::PathBuf::from("/home/op"));
    ground
}

#[test]
fn a_denial_that_covers_the_chat_s_ground_refuses_it_naming_the_file_and_the_word() {
    let ground = ground_of("/plane/ws/repo");
    for path in [
        "/",
        "/home/op",
        "/plane",
        "/plane/ws",
        "/plane/ws/repo",
        "/plane/ws/repo/..",
        "/plane/ws/repo/nowhere/../..",
    ] {
        let denied = [later_code(path, "/plane/.claude/settings.json", "./")];
        assert_eq!(
            covering(&denied, &ground),
            Some(NotStarted::CoversItsGround {
                path: std::path::PathBuf::from(path),
                class: Class::LaterCode,
                named: Some(Named {
                    file: std::path::PathBuf::from("/plane/.claude/settings.json"),
                    word: "./".to_owned(),
                }),
                within: None,
            }),
            "{path}"
        );
    }
    assert_eq!(
        covering(
            &[later_code("/plane/ws", "/plane/ws/.mcp.json", "../ws")],
            &ground
        )
        .map(|it| it.to_string()),
        Some(
            "this project runs every chat sandboxed, and `../ws` in /plane/ws/.mcp.json reads as \
             a script purlis keeps this chat from changing, which would leave it unable to \
             write /plane/ws, so nothing was started. Change that word in /plane/ws/.mcp.json, \
             or start this chat without the sandbox from the new-chat picker."
                .to_owned()
        )
    );
    assert_eq!(
        covering(
            &[one(Class::Vaults, "/home/op", Access::ReadWrite)],
            &ground
        )
        .map(|it| it.to_string()),
        Some(
            "this project runs every chat sandboxed, and its vaults rules would keep the chat \
             from writing /home/op, so nothing was started. Start this chat without the \
             sandbox from the new-chat picker."
                .to_owned()
        )
    );
}

#[test]
fn a_denial_below_or_beside_the_chat_s_ground_is_kept() {
    let ground = ground_of("/plane/ws/repo");
    let denied = [
        later_code("/plane/ws/repo/scripts/x.sh", "/f", "./scripts/x.sh"),
        later_code("/plane/other", "/f", "../other"),
        later_code("/home/op/.config/x", "/f", "~/.config/x"),
        one(Class::Integrity, "/plane/.charter/app", Access::Write),
    ];
    assert_eq!(covering(&denied, &ground), None);
}

/// A plane with the sandbox on whose `dir` has a Claude Code hook running `command`.
fn plane_with_hook(dir: &str, command: &str) -> tempfile::TempDir {
    let plane = plane_saying(ON_WITHOUT_CACHES);
    let at = plane.path().join(dir);
    std::fs::create_dir_all(at.join(".claude")).expect(".claude");
    let settings = serde_json::json!({"hooks": {"PostToolUse": [{"hooks": [
        {"type": "command", "command": command}
    ]}]}});
    std::fs::write(at.join(".claude/settings.json"), settings.to_string()).expect("settings");
    plane
}

#[test]
fn what_a_wrap_adds_is_held_to_the_same_ground() {
    // A compiler's own denials, after the neutral ones: the operator's Codex home where
    // `CODEX_HOME` is the home directory.
    let compiled = Compiled {
        homes: Homes {
            home: Some(std::path::PathBuf::from("/home/op")),
            codex: Some(std::path::PathBuf::from("/home/op")),
            codex_project: Some(std::path::PathBuf::from("/data/codex-homes/x")),
            ..Homes::default()
        },
        ..compiled(Denied::default(), Os::MacOs)
    };
    let form = Form::Codex(codex::wrap(&compiled).expect("wraps"));
    assert_eq!(
        covering(
            form.denied().expect("a wrap's own list"),
            &ground_of("/plane/ws/repo")
        ),
        Some(NotStarted::CoversItsGround {
            path: std::path::PathBuf::from("/home/op"),
            class: Class::LaterCode,
            named: None,
            within: None,
        })
    );
}

#[test]
fn a_codex_home_that_is_the_home_directory_refuses_the_chat() {
    let plane = plane_saying(ON);
    let root = plane.path().canonicalize().expect("the plane");
    let machine = Machine {
        env: crate::secrets::Env::of(&[("CODEX_HOME", "/home/op")]),
        ..machine(Os::MacOs)
    };
    assert_eq!(
        for_start(Harness::Codex, &root, &machine, &|_| true),
        Err(NotStarted::CoversItsGround {
            path: std::path::PathBuf::from("/home/op"),
            class: Class::LaterCode,
            named: None,
            within: None,
        })
    );
}

#[test]
fn a_config_that_names_the_project_refuses_the_chat_rather_than_starting_it_read_only() {
    let plane = plane_saying(ON);
    let root = plane.path().canonicalize().expect("the plane");
    let clone = root.join("ws/repo");
    std::fs::create_dir_all(clone.join(".git")).expect("a clone");
    std::fs::write(clone.join(".git/config"), "[core]\nhooksPath = ../..\n").expect("config");
    // Every harness purlis sandboxes: the refusal is the neutral policy's, before any compiler.
    for harness in Harness::ALL
        .into_iter()
        .filter(|harness| never_on(*harness, Os::MacOs).is_none())
    {
        let started = for_start(harness, &root, &machine(Os::MacOs), &|_| true);
        let Err(NotStarted::CoversItsGround { path, named, .. }) = started else {
            panic!("{harness:?}: {started:?}");
        };
        assert_eq!(path, clone.join("../.."));
        assert_eq!(
            named,
            Some(Named {
                file: clone.join(".git/config"),
                word: "../..".to_owned(),
            })
        );
    }
}

#[test]
fn a_hook_word_that_names_the_chat_s_own_folder_refuses_a_chat_there_only() {
    let plane = plane_with_hook("ws/repo", "make -C ./tools/");
    let root = plane.path().canonicalize().expect("the plane");
    let tools = root.join("ws/repo/tools");
    std::fs::create_dir_all(&tools).expect("tools");
    let applied =
        compiled_anyway(Harness::ClaudeCode, &root, &machine(Os::MacOs)).expect("compiles");
    let words = Words {
        program: "claude".to_owned(),
        command: Vec::new(),
        armed: Vec::new(),
        charters: Vec::new(),
    };
    let line = |cwd: &std::path::Path| {
        applied.line(
            words.clone(),
            &At {
                cwd: Some(cwd),
                ..At::default()
            },
        )
    };
    assert_eq!(
        line(&tools),
        Err(NotStarted::CoversItsGround {
            path: tools.clone(),
            class: Class::LaterCode,
            named: Some(Named {
                file: root.join("ws/repo/.claude/settings.json"),
                word: "./tools/".to_owned(),
            }),
            within: None,
        }
        .to_string())
    );
    assert!(line(&root.join("ws/repo")).is_ok());
}

#[test]
fn a_jq_hook_leaves_the_chat_s_folder_and_its_workspace_writable() {
    // #1327, as seen: the hook below made every chat in the project read-only.
    let plane = plane_with_hook(
        "workspaces/w/repo",
        "jq -r '.tool_response.filePath // .tool_input.file_path' | xargs -r prettier --write",
    );
    let root = plane.path().canonicalize().expect("the plane");
    let workspace = root.join("workspaces/w");
    let clone = workspace.join("repo");
    let applied =
        compiled_anyway(Harness::ClaudeCode, &root, &machine(Os::MacOs)).expect("compiles");
    let Form::ClaudeCode(settings) = applied.form() else {
        panic!("a Claude Code form");
    };
    // Whether a rule, `/`-rooted or not, holds `path`: a `**/<name>` rule names a name at any
    // depth and holds no folder by itself; any other is the folder it names and what is below.
    let holds = |rule: &str, path: &std::path::Path| {
        if rule.starts_with("**/") {
            return false;
        }
        let folder = rule.trim_end_matches("**").trim_end_matches('/');
        folder.is_empty() || path.starts_with(folder)
    };
    let written = |path: &std::path::Path| {
        let by_sandbox = deny_write(settings).iter().any(|it| holds(it, path));
        let by_tools = settings.deny.iter().any(|rule| {
            rule.strip_prefix("Edit(")
                .and_then(|it| it.strip_suffix(')'))
                .is_some_and(|it| holds(it.strip_prefix('/').unwrap_or(it), path))
        });
        !by_sandbox && !by_tools
    };
    // The helper holds what #1327 wrote.
    assert!(holds("/", &root) && holds("/**", &root) && holds("**", &root));
    assert!(holds(&format!("{}/**", workspace.display()), &clone));
    for folder in [&clone, &workspace, &root] {
        assert!(
            written(folder),
            "{} is denied: {:?} {:?}",
            folder.display(),
            deny_write(settings),
            settings.deny
        );
    }
    assert!(!deny_write(settings).iter().any(|it| it == "/"));
    assert!(
        !settings
            .deny
            .iter()
            .any(|it| it == "Edit(//)" || it == "Edit(//**)")
    );
    let words = Words {
        program: "claude".to_owned(),
        command: Vec::new(),
        armed: Vec::new(),
        charters: Vec::new(),
    };
    assert!(
        applied
            .line(
                words,
                &At {
                    cwd: Some(&clone),
                    ..At::default()
                }
            )
            .is_ok()
    );
}

#[cfg(unix)]
#[test]
fn a_denial_through_a_link_and_a_missing_folder_is_still_held_to_the_ground() {
    let base = tempfile::tempdir().expect("a base");
    let real_dir = base.path().join("real");
    std::fs::create_dir_all(real_dir.join("x")).expect("a real folder");
    std::os::unix::fs::symlink(&real_dir, base.path().join("link")).expect("a link");
    let denied = [later_code(
        base.path()
            .join("link/missing/..")
            .to_str()
            .expect("a path"),
        "/f",
        "../..",
    )];
    let ground = [real_dir.join("x").canonicalize().expect("the folder")];
    assert!(covering(&denied, &ground).is_some());
}

#[test]
fn a_config_purlis_could_not_read_through_refuses_the_chat() {
    let named = Named {
        file: std::path::PathBuf::from("/plane/ws/.claude/settings.json"),
        word: "./d64".to_owned(),
    };
    let compiled = compiled(
        Denied {
            unread: Some(named.clone()),
            ..Denied::default()
        },
        Os::MacOs,
    );
    let compile = compiler(Harness::ClaudeCode).expect("a compiler");
    let refused = compile_checked(
        compile,
        &compiled,
        std::path::Path::new("/plane"),
        &machine(Os::MacOs),
    )
    .map(|_| ())
    .expect_err("refused");
    assert_eq!(refused, NotStarted::Unread(named));
    assert_eq!(
        refused.to_string(),
        "this project runs every chat sandboxed, and a command in \
         /plane/ws/.claude/settings.json changes folder or names scripts more often than purlis \
         follows, from `./d64` on, so purlis cannot tell what it runs, and nothing was started. \
         Move that command into a script of its own, or start this chat without the sandbox \
         from the new-chat picker."
    );
}

/// #1333's rule, which every brokered write asks (ADR 0067 §1 as amended): a chat never changes
/// the sandbox, at any level, through purlisd.
#[test]
fn a_brokered_write_that_changes_any_sandbox_key_is_told_it_does() {
    let on = "schema = 1\n[sandbox]\nmode = \"on\"\nhosts = [\"a.example\"]\n";
    // Another key, and a comment, change no sandbox key.
    assert!(!changes_a_sandbox_key(
        Some(on),
        &format!("{on}\n[memory]\nshare = \"local\"\n# a note\n")
    ));
    assert!(!changes_a_sandbox_key(None, "schema = 1\n"));
    for after in [
        "schema = 1\n[sandbox]\nmode = \"on\"\nhosts = [\"a.example\", \"10.0.0.5\"]\n",
        "schema = 1\n[sandbox]\nmode = \"on\"\n",
        "schema = 1\n[sandbox]\nmode = \"on\"\nhosts = [\"a.example\"]\negress = []\n",
        "schema = 1\n",
        "not toml [",
    ] {
        assert!(changes_a_sandbox_key(Some(on), after), "{after}");
    }
    // This machine's file, made where there was none, with hosts of its own.
    assert!(changes_a_sandbox_key(
        None,
        "[sandbox]\nhosts = [\"10.0.0.5\"]\n"
    ));
    // A file that was not TOML is unknown, so a write over it is never let through as no change.
    assert!(changes_a_sandbox_key(Some("not toml ["), "schema = 1\n"));
}

/// Review of #1341, 8: a `[[forge]]` host is taken as a project's own host is, so it never
/// reaches this machine or a metadata service.
#[test]
fn a_forge_host_is_taken_as_a_host_is() {
    for host in ["127.0.0.1", "localhost", "169.254.169.254", "0x7f.1"] {
        let hosts = hosts_of(
            &[Preset::Forge],
            Some(&format!(
                "[[forge]]\nkind = \"gitlab\"\nhost = \"{host}\"\n"
            )),
        );
        assert!(!hosts.contains(&host.to_owned()), "{host}: {hosts:?}");
    }
}

/// Review of #1341, 8: the Notice names a project's `[[forge]]` hosts with its own, while the
/// `forge` preset lets every chat reach them.
#[test]
fn the_hosts_a_teammate_is_told_of_include_the_forge_hosts() {
    let plane = Plane::of(Some(
        "[sandbox]\nmode = \"on\"\nhosts = [\"10.0.0.5:6443\"]\n\n[[forge]]\nkind = \"gitlab\"\n\
         host = \"git.example.org:8443\"\n",
    ));
    assert_eq!(
        plane.granted_hosts(&policy::Locks::none()),
        ["10.0.0.5:6443", "git.example.org"]
    );
    let without = Plane::of(Some(
        "[sandbox]\nmode = \"on\"\negress = []\n\n[[forge]]\nkind = \"gitlab\"\n\
         host = \"git.example.org\"\n",
    ));
    assert_eq!(
        without.granted_hosts(&policy::Locks::none()),
        Vec::<String>::new()
    );
}

/// Review of #1341, round 3: a chat can write all of a clone's git state, so this machine's
/// file grants only what you confirmed in Settings. A host a chat (or a teammate's commit) put
/// there grants nothing until it is confirmed.
#[test]
fn this_machine_s_hosts_grant_only_what_you_confirmed() {
    let plane = plane_saying("[sandbox]\nmode = \"on\"\negress = []\n");
    std::fs::write(
        plane.path().join("charter.local.toml"),
        "[sandbox]\nhosts = [\"10.0.0.6\", \"10.0.0.7\"]\n",
    )
    .expect("written by a chat");
    assert_eq!(hosts::personal(plane.path()), Vec::<hosts::Host>::new());
    local::confirm_host(plane.path(), "10.0.0.7").expect("confirmed");
    assert_eq!(
        hosts::personal(plane.path()),
        [hosts::Host::parse("10.0.0.7").unwrap()]
    );
}

/// Review of #1341, 6: this machine's file is never read through a link.
#[cfg(unix)]
#[test]
fn this_machine_s_hosts_through_a_link_grant_nothing() {
    let plane = plane_saying("[sandbox]\nmode = \"on\"\negress = []\n");
    let elsewhere = tempfile::tempdir().expect("elsewhere");
    let target = elsewhere.path().join("hosts.toml");
    std::fs::write(&target, "[sandbox]\nhosts = [\"10.0.0.6\"]\n").expect("target");
    std::os::unix::fs::symlink(&target, plane.path().join("charter.local.toml")).expect("link");
    assert_eq!(hosts::personal(plane.path()), Vec::<hosts::Host>::new());
}

// -------------------------------------------------------------------------------------
// What a project's sandbox widens past its hosts (spec #1330, #1337): the project's own
// package caches with the toolchains preset, the certificate check only where the project
// turns it on
// -------------------------------------------------------------------------------------

use std::path::{Path as StdPath, PathBuf as StdPathBuf};

/// A Mac whose home is `/Users/op`, with `env`, and a terminal's `PATH` unless `env` says one.
fn mac_with(env: &[(&str, &str)]) -> Machine {
    let mut vars = vec![(
        "PATH",
        "/Users/op/.cargo/bin:/Users/op/go/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin",
    )];
    vars.retain(|(key, _)| !env.iter().any(|(set, _)| set == key));
    vars.extend_from_slice(env);
    Machine {
        env: crate::secrets::Env::of(&vars),
        home: Some(StdPathBuf::from("/Users/op")),
        os: Os::MacOs,
    }
}

const PROJECT: &str = "/Users/op/project";

fn policy_of(presets: &[Preset], certificate_checks: bool) -> Policy {
    Policy {
        egress: presets.to_vec(),
        hosts: vec![],
        certificate_checks,
        personas: Default::default(),
    }
}

/// What `harness` starts under in the project at `root` with `policy`, on `machine`: compiled
/// as a start compiles it, with no `charter.toml` written.
fn compiled_for(harness: Harness, policy: &Policy, root: &StdPath, machine: &Machine) -> Applied {
    applied_for(harness, policy, &Plane::of(None), root, machine).expect("compiles")
}

fn under_presets(harness: Harness, presets: &[Preset], machine: &Machine) -> Applied {
    compiled_for(
        harness,
        &policy_of(presets, false),
        StdPath::new(PROJECT),
        machine,
    )
}

/// The project at `root`'s cache home on `machine`, where its sandboxed chats' caches live.
fn cache_home(machine: &Machine, root: &StdPath) -> StdPathBuf {
    Homes::charter_data(machine)
        .expect("a data home")
        .join("cache-homes")
        .join(Homes::project_key(root))
}

fn claude_sandbox(applied: &Applied) -> &serde_json::Value {
    match applied.form() {
        Form::ClaudeCode(settings) => &settings.sandbox,
        other => panic!("not Claude Code's: {other:?}"),
    }
}

/// The Seatbelt profile `applied` wraps a chat in, for a wrapped harness.
fn wrap_profile(applied: &Applied) -> String {
    let cwd = StdPath::new(PROJECT);
    let tmp = StdPath::new("/Users/op/project-tmp");
    match applied.form() {
        Form::Opencode(wrap) => opencode::profile(wrap, cwd, tmp, 4040, None),
        Form::Codex(wrap) => codex::profile(wrap, cwd, tmp, 4040, None),
        Form::ClaudeCode(_) => panic!("Claude Code is not wrapped"),
    }
    .expect("a profile")
}

/// Every folder a chat writes whole in a cache home, in the order they are granted.
const TREES: [&str; 11] = [
    "cargo/registry",
    "cargo/git/checkouts",
    "npm",
    "pip",
    "go-mod",
    "go-build",
    "gradle",
    "yarn",
    "yarn-berry",
    "pnpm-store",
    "pnpm-cache",
];

/// The files a chat writes at the top of the project's cargo home.
const CARGO_FILES: [&str; 6] = [
    ".package-cache",
    ".package-cache-mutate",
    ".global-cache",
    ".global-cache-wal",
    ".global-cache-shm",
    ".global-cache-journal",
];

#[test]
fn the_toolchains_preset_gives_a_chat_its_projects_own_caches_and_only_with_it() {
    let machine = mac_with(&[]);
    let home = cache_home(&machine, StdPath::new(PROJECT));
    let on = under_presets(Harness::ClaudeCode, &[Preset::Toolchains], &machine);
    let mut expected: Vec<String> = TREES
        .iter()
        .map(|tree| home.join(tree).display().to_string())
        .collect();
    expected.extend(
        CARGO_FILES
            .iter()
            .map(|file| home.join("cargo").join(file).display().to_string()),
    );
    expected.push(format!("{}/cargo/git/db/*/**", home.display()));
    assert_eq!(
        claude_sandbox(&on)["filesystem"]["allowWrite"],
        serde_json::json!(expected)
    );
    // Every folder a chat may write is the project's own; none of the person's caches is.
    let writable = on.writable();
    assert_eq!(writable.len(), TREES.len() + 1 + CARGO_FILES.len());
    for path in &writable {
        assert!(
            path.starts_with(&home),
            "{} is not the project's",
            path.display()
        );
    }
    for person in [
        "/Users/op/.cargo",
        "/Users/op/.npm",
        "/Users/op/Library/Caches",
        "/Users/op/.cache",
        "/Users/op/go",
        "/Users/op/.gradle",
        "/Users/op/.yarn",
        "/Users/op/Library/pnpm",
    ] {
        assert!(
            !writable.iter().any(|path| path.starts_with(person)),
            "{person} is written"
        );
    }

    let off = under_presets(
        Harness::ClaudeCode,
        &[Preset::ModelProviders, Preset::Forge],
        &machine,
    );
    assert_eq!(claude_sandbox(&off)["filesystem"].get("allowWrite"), None);
    assert_eq!(off.writable(), Vec::<StdPathBuf>::new());

    for harness in [Harness::Opencode, Harness::Codex] {
        let on = wrap_profile(&under_presets(harness, &[Preset::Toolchains], &machine));
        let off = wrap_profile(&under_presets(harness, &[Preset::Forge], &machine));
        for tree in TREES {
            let tree = home.join(tree);
            let grant = format!("  (subpath \"{}\")\n", tree.display());
            assert!(
                on.contains(&grant),
                "{harness:?} does not grant {tree:?}:\n{on}"
            );
            // Pinned, so it is never removed, renamed or swapped for a link.
            let pin = format!("(deny file-write* (literal \"{}\"))\n", tree.display());
            assert!(on.contains(&pin), "{harness:?} does not pin {tree:?}");
            assert!(
                on.find(&pin) > on.find(&grant),
                "{harness:?}: pinned before granted"
            );
            // The later-code names stay denied at any depth inside each.
            let regex = format!(
                "^{}/(.*/)?\\.git/config(/.*)?$",
                seatbelt::escaped(&tree.display().to_string())
            );
            let planted = format!(
                "(deny file-write* (regex {}))",
                seatbelt::string(&regex).expect("a string")
            );
            assert!(on.contains(&planted), "{harness:?}, {tree:?}:\n{on}");
        }
        for file in CARGO_FILES {
            let grant = format!(
                "  (literal \"{}\")\n",
                home.join("cargo").join(file).display()
            );
            assert!(on.contains(&grant), "{harness:?} does not grant {file}");
        }
        assert!(!off.contains("cache-homes"), "{harness:?}:\n{off}");
    }
}

#[test]
fn a_cargo_bare_repository_is_written_inside_and_never_made_moved_or_run() {
    let machine = mac_with(&[]);
    let db = cache_home(&machine, StdPath::new(PROJECT)).join("cargo/git/db");
    let on = under_presets(Harness::ClaudeCode, &[Preset::Toolchains], &machine);
    let sandbox = claude_sandbox(&on);
    let allowed = sandbox["filesystem"]["allowWrite"]
        .as_array()
        .expect("a list");
    assert!(!allowed.contains(&serde_json::json!(db.display().to_string())));
    let denied = sandbox["filesystem"]["denyWrite"]
        .as_array()
        .expect("a list");
    for glob in [
        format!("{}/*/config", db.display()),
        format!("{}/*/hooks", db.display()),
    ] {
        assert!(
            denied.contains(&serde_json::json!(glob)),
            "{glob} is not denied"
        );
    }
    // A `.git` at each folder's top, which pins the folder in Claude Code's profile.
    for tree in TREES {
        let pin = format!(
            "{}/.git",
            db.parent()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join(tree)
                .display()
        );
        assert!(
            denied.contains(&serde_json::json!(pin)),
            "{pin} is not denied"
        );
    }
    for harness in [Harness::Opencode, Harness::Codex] {
        let profile = wrap_profile(&under_presets(harness, &[Preset::Toolchains], &machine));
        let escaped = seatbelt::escaped(&db.display().to_string());
        let inside = format!(
            "  (regex {})\n",
            seatbelt::string(&format!("^{escaped}/[^/]+/.+$")).expect("a string")
        );
        assert!(profile.contains(&inside), "{harness:?}:\n{profile}");
        let pin = format!("(deny file-write* (literal \"{}\"))", db.display());
        let run = format!(
            "(deny file-write* (regex {}))",
            seatbelt::string(&format!("^{escaped}/[^/]+/(config|hooks)(/.*)?$")).expect("a string")
        );
        for rule in [&pin, &run] {
            assert!(
                profile.find(rule.as_str()) > profile.find(&inside),
                "{harness:?}: {rule}"
            );
        }
    }
}

#[test]
fn each_tool_is_pointed_at_its_projects_cache_and_the_folders_are_made_before_the_chat() {
    let base = tempfile::tempdir().expect("a base");
    let project = base.path().join("project");
    std::fs::create_dir_all(&project).expect("a project");
    let project = project.canonicalize().expect("the project");
    let data = base.path().join("data");
    let data_text = data.display().to_string();
    let machine = mac_with(&[(crate::datahome::HOME_VAR, data_text.as_str())]);
    let applied = compiled_for(
        Harness::ClaudeCode,
        &policy_of(&[Preset::Toolchains], false),
        &project,
        &machine,
    );
    let line = applied
        .line(
            Words {
                program: "claude".to_owned(),
                command: Vec::new(),
                armed: Vec::new(),
                charters: Vec::new(),
            },
            &At {
                cwd: Some(&project),
                ..At::default()
            },
        )
        .expect("starts");
    let home = cache_home(&machine, &project);
    for (var, dir) in [
        ("CARGO_HOME", "cargo"),
        ("npm_config_cache", "npm"),
        ("NPM_CONFIG_CACHE", "npm"),
        ("PIP_CACHE_DIR", "pip"),
        ("GOMODCACHE", "go-mod"),
        ("GOCACHE", "go-build"),
        ("GRADLE_USER_HOME", "gradle"),
        ("YARN_CACHE_FOLDER", "yarn"),
        ("YARN_GLOBAL_FOLDER", "yarn-berry"),
        ("pnpm_config_store_dir", "pnpm-store"),
        ("pnpm_config_cache_dir", "pnpm-cache"),
        (concat!("npm", "_config_store_dir"), "pnpm-store"),
        (concat!("npm", "_config_cache_dir"), "pnpm-cache"),
    ] {
        let value = home.join(dir).display().to_string();
        assert!(
            line.env.contains(&(var.to_owned(), value.clone())),
            "{var}={value} is not in {:?}",
            line.env
        );
        assert!(home.join(dir).is_dir(), "{dir} was not made");
    }
    assert!(home.join("cargo/git/db").is_dir());
    let config = std::fs::read_to_string(home.join("cargo/config.toml")).expect("a config");
    assert!(config.starts_with("# Written by purlis"), "{config}");
}

#[cfg(unix)]
#[test]
fn a_link_planted_in_the_cache_home_is_taken_out_unfollowed_and_the_chat_starts() {
    let base = tempfile::tempdir().expect("a base");
    let project = base.path().join("project");
    std::fs::create_dir_all(&project).expect("a project");
    let project = project.canonicalize().expect("the project");
    let elsewhere = base.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("elsewhere");
    std::fs::write(elsewhere.join("config.toml"), "kept").expect("a file elsewhere");
    let data = base.path().join("data");
    let data_text = data.display().to_string();
    let machine = mac_with(&[(crate::datahome::HOME_VAR, data_text.as_str())]);
    let home = cache_home(&machine, &project);
    let policy = policy_of(&[Preset::Toolchains], false);
    let words = || Words {
        program: "claude".to_owned(),
        command: Vec::new(),
        armed: Vec::new(),
        charters: Vec::new(),
    };
    let at = At {
        cwd: Some(&project),
        ..At::default()
    };

    // A link planted where a cache folder goes, before the sandbox is compiled: taken out, and
    // the grant names the folder purlis makes, never where the link pointed.
    std::fs::create_dir_all(&home).expect("a cache home");
    std::os::unix::fs::symlink(&elsewhere, home.join("npm")).expect("a link");
    let applied = compiled_for(Harness::ClaudeCode, &policy, &project, &machine);
    assert!(
        std::fs::symlink_metadata(home.join("npm")).is_err(),
        "the link stayed"
    );
    let writable = applied.writable();
    assert!(writable.contains(&home.join("npm")), "{writable:?}");
    assert!(
        !writable.iter().any(|path| path.starts_with(&elsewhere)),
        "{writable:?}"
    );

    // Links planted after it was compiled, at a folder and at cargo's config: taken out at the
    // start, nothing written through them, and the chat starts.
    std::fs::create_dir_all(home.join("cargo/git")).expect("cargo's git");
    std::os::unix::fs::symlink(&elsewhere, home.join("cargo/git/db")).expect("a link");
    std::os::unix::fs::symlink(
        elsewhere.join("config.toml"),
        home.join("cargo/config.toml"),
    )
    .expect("a link");
    applied.line(words(), &at).expect("starts");
    for folder in ["npm", "cargo/git/db"] {
        let meta = std::fs::symlink_metadata(home.join(folder)).expect("made");
        assert!(meta.is_dir() && !meta.file_type().is_symlink(), "{folder}");
    }
    let config = std::fs::symlink_metadata(home.join("cargo/config.toml")).expect("written");
    assert!(config.is_file() && !config.file_type().is_symlink());
    assert_eq!(
        std::fs::read_to_string(elsewhere.join("config.toml")).expect("read"),
        "kept"
    );
    assert_eq!(std::fs::read_dir(&elsewhere).expect("read").count(), 1);
}

#[test]
fn a_cache_link_purlis_cannot_take_out_is_named_in_the_refusal() {
    let path = std::path::Path::new("/data/cache-homes/k/npm");
    assert_eq!(
        caches_linked(path, "after"),
        "this project runs every chat sandboxed, and /data/cache-homes/k/npm of the project's \
         package caches is a link purlis could not take out (after making them), so nothing \
         was started. Remove that link and start the chat again."
    );
}

#[test]
fn a_projects_cargo_home_keeps_where_crates_come_from_and_nothing_else() {
    let person = r#"
[registries.corp]
index = "sparse+https://crates.corp.example/index/"
token = "a-secret"
credential-provider = "cargo:token-from-stdout /usr/local/bin/creds"

[registry]
default = "corp"
token = "another-secret"
global-credential-providers = ["/usr/local/bin/creds"]

[source.crates-io]
replace-with = "vendored"

[source.vendored]
directory = "vendor"

[build]
rustc-wrapper = "/usr/local/bin/sccache"

[target.x86_64-apple-darwin]
runner = "/usr/local/bin/run"

[alias]
b = "build"
"#;
    let seeded = cargo_config_text(person);
    let parsed: toml::Table = seeded.parse().expect("TOML");
    let expected: toml::Table = r#"
[registries.corp]
index = "sparse+https://crates.corp.example/index/"

[registry]
default = "corp"

[source.crates-io]
replace-with = "vendored"

[source.vendored]
directory = "vendor"
"#
    .parse()
    .expect("TOML");
    assert_eq!(parsed, expected);
    for word in ["secret", "creds", "sccache", "runner", "alias"] {
        assert!(!seeded.contains(word), "{word} is in:\n{seeded}");
    }
    // Nothing purlis can read is nothing.
    assert_eq!(
        cargo_config_text("not toml [")
            .parse::<toml::Table>()
            .expect("TOML"),
        toml::Table::new()
    );
}

fn cargo_config_text(person: &str) -> String {
    caches::cargo_config(person)
}

#[test]
fn certificate_checks_are_off_unless_the_project_turns_them_on() {
    assert_eq!(
        said("[sandbox]\nmode = \"on\"\n")
            .policy
            .map(|policy| policy.certificate_checks),
        Some(false)
    );
    let on = said("[sandbox]\nmode = \"on\"\ncertificate-checks = true\n");
    assert_eq!(
        on.policy.map(|policy| policy.certificate_checks),
        Some(true)
    );
    assert!(on.refused.is_empty(), "{:?}", on.refused);
    let typo = said("[sandbox]\nmode = \"on\"\ncertificate-checks = \"yes\"\n");
    assert_eq!(
        typo.policy.map(|policy| policy.certificate_checks),
        Some(false)
    );
    assert_eq!(typo.refused, [Refusal::CertificateChecksNotABool]);
    assert_eq!(
        typo.refused[0].to_string(),
        "sandbox.certificate-checks in charter.toml is not true or false; certificate checks \
         stay off"
    );

    let machine = mac_with(&[]);
    for (policy, on) in [
        // The forge preset alone no longer turns it on.
        (policy_of(&Preset::ALL, false), false),
        (policy_of(&[Preset::ModelProviders], true), true),
    ] {
        let applied = compiled_for(
            Harness::ClaudeCode,
            &policy,
            StdPath::new(PROJECT),
            &machine,
        );
        // Never left out: a user's `true` would merge in.
        assert_eq!(
            claude_sandbox(&applied)["enableWeakerNetworkIsolation"],
            serde_json::json!(on),
            "{policy:?}"
        );
        for harness in [Harness::Opencode, Harness::Codex] {
            let profile = wrap_profile(&compiled_for(
                harness,
                &policy,
                StdPath::new(PROJECT),
                &machine,
            ));
            assert_eq!(
                profile.contains("(allow mach-lookup (global-name \"com.apple.trustd.agent\"))\n"),
                on,
                "{harness:?}, {policy:?}:\n{profile}"
            );
            for service in ["SecurityServer", "securityd", "com.apple.security"] {
                assert!(!profile.contains(service), "{service}:\n{profile}");
            }
        }
    }
}

#[test]
fn the_ground_refusal_names_the_repo_and_the_workspace_its_config_sits_in() {
    // #1356: a word in a clone's config, then in a workspace's own, then in the project's.
    for (dir, place) in [
        (
            "workspaces/w/repo",
            Some("(in the repo workspaces/w/repo of workspace w)"),
        ),
        ("workspaces/w", Some("(in workspace w)")),
        ("ws/repo", Some("(in the repo ws/repo)")),
        ("", None),
    ] {
        let plane = plane_with_hook(dir, "make -C ./tools/");
        let root = plane.path().canonicalize().expect("the plane");
        let at = root.join(dir);
        if dir.ends_with("repo") {
            std::fs::create_dir_all(at.join(".git")).expect("a clone");
        }
        let tools = at.join("tools");
        std::fs::create_dir_all(&tools).expect("tools");
        let applied =
            compiled_anyway(Harness::ClaudeCode, &root, &machine(Os::MacOs)).expect("compiles");
        let words = Words {
            program: "claude".to_owned(),
            command: Vec::new(),
            armed: Vec::new(),
            charters: Vec::new(),
        };
        let refused = applied
            .line(
                words,
                &At {
                    cwd: Some(&tools),
                    ..At::default()
                },
            )
            .expect_err("refused");
        let file = at.join(".claude/settings.json");
        match place {
            Some(place) => assert!(
                refused.contains(&format!("in {} {place} reads", file.display())),
                "{dir}: {refused}"
            ),
            None => assert!(
                refused.contains(&format!("in {} reads", file.display())),
                "{dir}: {refused}"
            ),
        }
    }
}

#[cfg(unix)]
#[test]
fn a_missing_folder_that_differs_from_the_ground_only_in_case_covers_it_where_case_folds() {
    // #1356: on a volume that folds case, `NEW` and `new` are one folder before either exists.
    let base = tempfile::tempdir().expect("a base");
    let base = base.path().canonicalize().expect("the base");
    std::fs::create_dir_all(base.join("plane")).expect("the plane");
    let folds = std::fs::metadata(base.join("PLANE")).is_ok();
    let ground = [base.join("plane/new/repo")];
    for written in ["plane/NEW", "PLANE/new/repo", "Plane/New/Repo/x/.."] {
        let denied = [later_code(
            base.join(written).to_str().expect("a path"),
            "/f",
            written,
        )];
        assert_eq!(covering(&denied, &ground).is_some(), folds, "{written}");
    }
    // Beside the ground in any case is still beside it.
    let denied = [later_code(
        base.join("PLANE/newer").to_str().expect("a path"),
        "/f",
        "../newer",
    )];
    assert_eq!(covering(&denied, &ground), None);
}

#[cfg(target_os = "macos")]
#[test]
fn a_denial_in_the_data_volume_spelling_is_held_to_the_ground_and_both_ways() {
    // #1356: macOS reaches a firmlinked folder (`/Users`, `/private`, …) by two names, and
    // resolving either leaves its spelling as it is.
    let base = tempfile::tempdir().expect("a base");
    let base = base.path().canonicalize().expect("the base");
    std::fs::create_dir_all(base.join("plane/ws")).expect("the plane");
    let data = std::path::Path::new("/System/Volumes/Data")
        .join(base.strip_prefix("/").expect("absolute"));
    assert!(data.join("plane").is_dir(), "{}", data.display());
    let plain = base.join("plane");
    let spelled = data.join("plane");
    for (denial, ground) in [(&spelled, &plain), (&plain, &spelled)] {
        let denied = [later_code(denial.to_str().expect("a path"), "/f", "x")];
        assert!(
            covering(&denied, &[ground.join("ws")]).is_some(),
            "{} over {}",
            denial.display(),
            ground.display()
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn a_denied_path_is_denied_under_its_data_volume_spelling_too() {
    // #1356: as a path through a link is denied by both names (FD-27).
    for (written, twin) in [
        (
            "/Users/op/tools/x.sh",
            "/System/Volumes/Data/Users/op/tools/x.sh",
        ),
        ("/System/Volumes/Data/Users/op/y.sh", "/Users/op/y.sh"),
    ] {
        let denied = Denied {
            paths: vec![later_code(written, "/f", "x")],
            ..Denied::default()
        };
        let settings = claude::settings(&compiled(denied, Os::MacOs)).expect("compiles");
        for name in [written, twin] {
            assert!(deny_write(&settings).iter().any(|it| it == name), "{name}");
            assert!(
                settings.deny.contains(&format!("Edit(/{name})")),
                "{name}: {:?}",
                settings.deny
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn no_directory_a_chat_searches_is_one_its_sandbox_lets_it_write() {
    // A directory on a chat's PATH that a chat can write is a program the next chat, the app's
    // own forge calls and the operator's terminal run without a sandbox. What a chat may write
    // is read from the compiled sandbox itself, for every harness with a compiler and every
    // preset on, so a grant added later is checked here without anyone listing it.
    const FINDER: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
    let home = StdPathBuf::from("/Users/op");
    for (machine, grants) in [
        (mac_with(&[]), true),
        (mac_with(&[("PATH", FINDER)]), true),
        (
            mac_with(&[
                ("CARGO_HOME", "/Users/op/tools/cargo"),
                ("GOBIN", "/Users/op/gobin"),
            ]),
            true,
        ),
        // A data home on the chat's PATH, or holding cargo's home: no cache home there.
        (
            mac_with(&[(crate::datahome::HOME_VAR, "/Users/op/.local/bin")]),
            false,
        ),
        // A data home that is the home itself still keeps the caches in a folder of their own.
        (mac_with(&[(crate::datahome::HOME_VAR, "/Users/op")]), true),
        (
            mac_with(&[(crate::datahome::HOME_VAR, "/Users/op/.cargo")]),
            false,
        ),
        (
            mac_with(&[(crate::datahome::HOME_VAR, "/Users/op/project/.data")]),
            false,
        ),
    ] {
        let path = machine.env.get("PATH").expect("a PATH");
        let mut searched: Vec<StdPathBuf> = std::env::split_paths(
            &crate::programs::chat_path_from(Some(std::ffi::OsStr::new(&path)), Some(&home), None)
                .expect("a chat's PATH"),
        )
        .collect();
        searched.extend(crate::programs::search_dirs_from(
            Some(std::ffi::OsStr::new(&path)),
            Some(&home),
        ));
        // And where a later program is run from whether or not this PATH names it: cargo's
        // home and its `bin` (its config names programs cargo runs), Go's `bin`, pnpm's home.
        searched.extend(caches::never_covered(&machine));
        searched.extend(
            [
                "/Users/op/.cargo",
                "/Users/op/.cargo/bin",
                "/Users/op/.cargo/config.toml",
                "/Users/op/.cargo/credentials.toml",
                "/Users/op/go/bin",
                "/Users/op/Library/pnpm",
            ]
            .map(StdPathBuf::from),
        );
        for harness in [Harness::ClaudeCode, Harness::Opencode, Harness::Codex] {
            let applied = under_presets(harness, &Preset::ALL, &machine);
            let caches = applied.caches.as_ref().map(|caches| caches.writable());
            assert_eq!(caches.is_some(), grants, "{harness:?}: {caches:?}");
            let mut writable = applied.writable();
            writable.extend(program::temp_roots(&[]));
            writable.push(StdPathBuf::from(PROJECT));
            for dir in &searched {
                for grant in &writable {
                    assert!(
                        !dir.starts_with(grant),
                        "{harness:?}: {} is searched, and a chat may write {}",
                        dir.display(),
                        grant.display()
                    );
                }
            }
        }
    }
}

/// What macOS tools read in place of `TMPDIR` (#1120, measured on macOS 26.2), each pointed into
/// the wrapped chat's own temp directory: `TMP` and `TEMP`; `xcrun`'s cache, which it otherwise
/// writes in the system's per-user temp folder; and clang's module cache, which clang and Swift
/// otherwise write in the per-user cache folder and fail without.
#[test]
fn a_wrapped_chat_points_what_tools_read_in_place_of_tmpdir_at_its_own_temp_directory() {
    let env: std::collections::BTreeMap<String, String> =
        seatbelt::env("http://127.0.0.1:1", Path::new("/c/.purlis-tmp/x"))
            .into_iter()
            .collect();
    for (key, value) in [
        ("TMPDIR", "/c/.purlis-tmp/x"),
        ("TMP", "/c/.purlis-tmp/x"),
        ("TEMP", "/c/.purlis-tmp/x"),
        ("xcrun_db", "/c/.purlis-tmp/x/xcrun_db"),
        (
            "CLANG_MODULE_CACHE_PATH",
            "/c/.purlis-tmp/x/clang-module-cache",
        ),
    ] {
        assert_eq!(env.get(key).map(String::as_str), Some(value), "{key}");
    }
}

#[test]
fn a_config_that_names_the_project_is_named_with_its_repo_and_workspace() {
    // #1356, before any chat's folder: the refusal at the start says where the config sits.
    let plane = tempfile::tempdir().expect("a plane");
    let root = plane.path().canonicalize().expect("the plane");
    std::fs::create_dir_all(root.join("workspaces/w/repo/.git")).expect("a clone");
    std::fs::create_dir_all(root.join("workspaces/v/.claude")).expect("a workspace");
    let compile = compiler(Harness::ClaudeCode).expect("a compiler");
    for (file, place) in [
        (
            "workspaces/w/repo/.claude/settings.json",
            " (in the repo workspaces/w/repo of workspace w) ",
        ),
        ("workspaces/v/.claude/settings.json", " (in workspace v) "),
        (".claude/settings.json", " "),
    ] {
        let file = root.join(file);
        let denied = Denied {
            paths: vec![later_code(
                root.to_str().expect("a path"),
                file.to_str().expect("a path"),
                "../..",
            )],
            ..Denied::default()
        };
        let refused = compile_checked(
            compile,
            &compiled(denied, Os::MacOs),
            &root,
            &machine(Os::MacOs),
        )
        .map(|_| ())
        .expect_err("refused")
        .to_string();
        assert!(
            refused.contains(&format!("in {}{place}reads as a script", file.display())),
            "{refused}"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn charters_own_wrap_denies_a_path_and_pins_its_folders_under_both_firmlink_names() {
    // #1356 review F1: the kernel may name a firmlinked folder either way, so a deny rule and
    // the pin on each folder between the chat's and the denied path carry both.
    let cwd = std::path::Path::new("/Users/op-1356/plane");
    let tmp = std::path::Path::new("/Users/op-1356/tmp");
    for written in [
        "/Users/op-1356/plane/tools/hook.sh",
        "/System/Volumes/Data/Users/op-1356/plane/tools/hook.sh",
    ] {
        let denied = [later_code(written, "/f", "./tools/hook.sh")];
        let profile = seatbelt::profile(&denied, &seatbelt::Own::default(), cwd, tmp, 4040, None)
            .expect("a profile");
        let rules: Vec<&str> = profile
            .lines()
            .filter(|line| {
                line.starts_with("(deny file-write* (subpath")
                    || line.starts_with("(deny file-write* (literal")
            })
            .collect();
        assert_eq!(
            rules,
            [
                "(deny file-write* (subpath \"/Users/op-1356/plane/tools/hook.sh\"))",
                "(deny file-write* (subpath \"/System/Volumes/Data/Users/op-1356/plane/tools/hook.sh\"))",
                "(deny file-write* (literal \"/Users/op-1356/plane\"))",
                "(deny file-write* (literal \"/System/Volumes/Data/Users/op-1356/plane\"))",
                "(deny file-write* (literal \"/Users/op-1356/tmp\"))",
                "(deny file-write* (literal \"/System/Volumes/Data/Users/op-1356/tmp\"))",
                "(deny file-write* (literal \"/Users/op-1356/plane/tools\"))",
                "(deny file-write* (literal \"/System/Volumes/Data/Users/op-1356/plane/tools\"))",
            ],
            "{written}:\n{profile}"
        );
    }
}

#[test]
fn the_firmlinks_are_the_system_s_list_and_the_known_one_together() {
    // #1356 review F2: an unreadable list falls back to the one compiled in, never to none.
    let known = firmlinks_from(None);
    for link in [
        "/Users",
        "/private",
        "/usr/local",
        "/Volumes",
        "/Applications",
    ] {
        assert!(known.contains(&std::path::PathBuf::from(link)), "{link}");
    }
    assert_eq!(known.len(), 18);
    assert_eq!(firmlinks_from(Some("")), known);
    let read = firmlinks_from(Some("/Extra\tExtra\n/Users\tUsers\n/\t\n"));
    assert!(read.contains(&std::path::PathBuf::from("/Extra")));
    assert_eq!(read.len(), 19);
}

/// A chat's environment as the app sets it: sandboxed or not, on `harness`.
fn chat_env(sandboxed: bool, harness: Option<&str>) -> impl Fn(&str) -> Option<String> {
    let harness = harness.map(str::to_owned);
    move |name: &str| match name {
        crate::hookwire::SANDBOXED_ENV if sandboxed => Some("1".to_owned()),
        crate::hookwire::HARNESS_ENV => harness.clone(),
        _ => None,
    }
}

#[test]
fn a_command_a_sandboxed_chat_runs_is_held_to_its_sandbox_on_every_harness() {
    for harness in [Some("claude-code"), Some("codex"), Some("opencode"), None] {
        assert!(
            writes_are_sandboxed_in(&chat_env(true, harness), Started::ByTheChat),
            "{harness:?}"
        );
        assert!(
            !writes_are_sandboxed_in(&chat_env(false, harness), Started::ByTheChat),
            "{harness:?}"
        );
    }
}

#[test]
fn a_hook_is_held_to_the_chat_s_sandbox_only_where_the_harness_s_sandbox_holds_its_hooks() {
    // #1421: Claude Code's sandbox confines its Bash tool, and its hooks run outside it, so a
    // refusal one meets there is not the sandbox's. Codex and opencode run whole inside the wrap.
    // The variable holds the profile's registry name, as a chat's start writes it.
    let hook = |harness| writes_are_sandboxed_in(&chat_env(true, harness), Started::ByTheHarness);
    assert!(!hook(Some("claude-code")));
    assert!(hook(Some("codex")));
    assert!(hook(Some("opencode")));
    // A harness this binary does not know, or none named: never blamed on a sandbox.
    assert!(!hook(Some("aider")));
    assert!(
        !hook(Some("claude")),
        "a kind word is not what the variable holds"
    );
    assert!(!hook(None));
    assert!(!writes_are_sandboxed_in(
        &chat_env(false, Some("codex")),
        Started::ByTheHarness
    ));
}

#[test]
fn every_harness_says_whether_its_sandbox_holds_what_it_starts() {
    for harness in crate::harness::Harness::ALL {
        let holds = harness.adapter().sandbox_holds_what_it_starts();
        // A wrap charter applies is around the whole harness; Claude Code's own is not.
        let wrapped = !matches!(harness, crate::harness::Harness::ClaudeCode);
        assert_eq!(holds, wrapped, "{harness:?}");
    }
}

#[test]
fn every_registry_name_a_chat_start_writes_names_its_harness() {
    // #1421 review: `PURLIS_HARNESS` holds the registry name, so the adapter must be found by it.
    for kind in crate::profiles::KINDS {
        let harness = harness_of_registry(kind.registry).expect(kind.registry);
        assert_eq!(harness.name(), kind.word);
    }
    assert_eq!(
        harness_of_registry("claude-code"),
        Some(crate::harness::Harness::ClaudeCode)
    );
}

#[cfg(target_os = "macos")]
#[test]
fn the_reporting_socket_s_folder_is_denied_under_every_name_it_has() {
    // #1418: as written, as the kernel resolves it, and on the data volume's other side.
    let (_plane, denied) = denied_with(None, Os::MacOs);
    let settings = claude::settings(&compiled(denied, Os::MacOs)).expect("compiles");
    let socket = std::path::Path::new("/tmp/purlis-1418-absent/app/hooks.sock");
    let mut reporting = settings.reporting_on(Some(socket));
    let names = [
        "/tmp/purlis-1418-absent/app",
        "/private/tmp/purlis-1418-absent/app",
        "/System/Volumes/Data/private/tmp/purlis-1418-absent/app",
    ];
    assert_eq!(
        deny_write(&reporting).split_off(deny_write(&settings).len()),
        names
    );
    let edits: Vec<String> = names
        .iter()
        .flat_map(|name| [format!("Edit(/{name})"), format!("Edit(/{name}/**)")])
        .collect();
    assert_eq!(reporting.deny.split_off(settings.deny.len()), edits);
    // The one socket a chat may reach is still the one the kernel names.
    assert_eq!(
        reporting.sandbox["network"]["allowUnixSockets"],
        serde_json::json!(["/private/tmp/purlis-1418-absent/app/hooks.sock"])
    );
}

#[cfg(unix)]
#[test]
fn a_case_swapped_link_beside_a_folder_does_not_make_its_volume_fold_case() {
    // #1418: the probe asks the folder itself, never what a link of the swapped name reaches.
    let base = tempfile::tempdir().expect("a base");
    let base = base.path().canonicalize().expect("the base");
    std::fs::create_dir_all(base.join("plane")).expect("the plane");
    if std::os::unix::fs::symlink(base.join("plane"), base.join("PLANE")).is_err() {
        // A volume that folds case already has `PLANE`: nothing to tell apart here.
        return;
    }
    let ground = [base.join("plane/new/repo")];
    let denied = [later_code(
        base.join("plane/NEW").to_str().expect("a path"),
        "/f",
        "NEW",
    )];
    assert_eq!(covering(&denied, &ground), None);
}

#[test]
fn a_profile_too_large_to_hand_seatbelt_is_refused_by_name() {
    // #1418: the profile goes in argv, which holds 1 MiB with the environment. One config at
    // its cap fits; a profile past `MOST_PROFILE` is refused with a reason, never an E2BIG.
    let cwd = std::path::Path::new("/opt/purlis-1418/plane");
    let tmp = std::path::Path::new("/opt/purlis-1418/tmp");
    let denied = |count: usize| -> Vec<Denial> {
        (0..count)
            .map(|at| {
                later_code(
                    &format!("/opt/purlis-1418/elsewhere/a-folder-name/{at:06}/hook.sh"),
                    "/f",
                    "x",
                )
            })
            .collect()
    };
    let one = seatbelt::profile(
        &denied(planted::MOST_NAMED),
        &seatbelt::Own::default(),
        cwd,
        tmp,
        4040,
        None,
    )
    .expect("one config at its cap");
    assert!(one.len() <= seatbelt::MOST_PROFILE, "{}", one.len());
    assert_eq!(
        seatbelt::profile(
            &denied(16 * planted::MOST_NAMED),
            &seatbelt::Own::default(),
            cwd,
            tmp,
            4040,
            None,
        ),
        Err(seatbelt::TOO_LARGE)
    );
    // The way out, after what happened, as every refusal names one.
    assert_eq!(
        seatbelt::not_started(
            "purlis could not start this chat.",
            seatbelt::TOO_LARGE,
            true
        ),
        "purlis could not start this chat. purlis cannot hand the sandbox a profile this long: \
         the project's configs name more paths than one chat's sandbox can hold, so nothing was \
         started. Have the configs name fewer scripts, or start this chat without the sandbox \
         from the new-chat picker."
    );
    assert_eq!(
        seatbelt::not_started("Lead.", seatbelt::CONTROL, true),
        format!("Lead. {}, so nothing was started.", seatbelt::CONTROL)
    );
}
