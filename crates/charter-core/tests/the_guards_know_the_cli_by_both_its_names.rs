//! The command line ships as `purlis`, and `charter` runs it too for the rename's window (RN-3,
//! #1255). A guard that knows the product's own binary only as `charter` would let every
//! refusal it makes be stepped around by typing the new name, so each one knows both — and still
//! knows `edm`, the name before `charter`, where it did.

use std::path::Path;

use charter_core::handoffguard::{self, Caller, REASON_SPELLING};
use charter_core::scaffold::settings::CONSENT_PATTERNS;
use charter_core::{consentspelling, floorguard, leakguard, personagate, proseguard};

fn words(line: &str) -> Vec<String> {
    line.split(' ').map(str::to_owned).collect()
}

/// The leak guard's answer outside any plane.
fn leak(cmd: &str) -> Option<String> {
    leakguard::leak_reason(cmd, "", Path::new("/nonexistent-plane-state"))
}

/// The consent-spelling guard in a project that carries no rule under the new name: every one
/// the plane in these tests has (none), so the purlis spelling is refused (RN-7 lifts it only
/// where the project carries the twin).
fn no_rules(cmd: &str) -> Option<String> {
    consentspelling::refusal(cmd, Path::new("/nonexistent-plane"), Path::new(""))
}

fn vault() -> String {
    format!(".charter/{}/db.json", "vaults")
}

#[test]
fn purlis_is_the_product_itself_however_it_is_spelt() {
    charter_core::unsteered!();
    for (prog, args) in [
        ("purlis", ""),
        ("PURLIS", ""),
        ("/Applications/purlis.app/Contents/MacOS/purlis", ""),
        ("python3", "python3 -m purlis status"),
        ("charter", ""),
        ("edm", ""),
    ] {
        assert!(
            leakguard::is_charter(prog, &words(args)),
            "{prog} {args} was not taken for the product"
        );
    }
    assert!(!leakguard::is_charter("purlisx", &[]));
}

#[test]
fn a_reveal_through_purlis_is_refused_as_one_through_charter_is() {
    charter_core::unsteered!();
    for cmd in [
        "purlis secret get db --reveal",
        "charter secret get db --reveal",
        "script -q /dev/null purlis secret get db --reveal",
    ] {
        assert!(leak(cmd).is_some(), "{cmd} was let through");
    }
    assert_eq!(leak("purlis secret get db"), None);
}

#[test]
fn purlis_secret_exec_is_a_wrapper_and_the_command_it_runs_is_read() {
    charter_core::unsteered!();
    for name in ["purlis", "charter"] {
        let cmd = format!("{name} secret exec devops -- cat {}", vault());
        assert!(leak(&cmd).is_some(), "{cmd} was let through");
    }
}

#[test]
fn landing_a_change_unattended_through_purlis_is_on_the_floor() {
    charter_core::unsteered!();
    for cmd in [
        "purlis change land",
        "/usr/local/bin/purlis change land",
        "python3 -m purlis change land",
        "charter change land",
        "edm change land",
    ] {
        assert!(
            floorguard::release_floor_reason(cmd, true).is_some(),
            "{cmd} was let through"
        );
    }
    assert_eq!(
        floorguard::release_floor_reason("purlis change show", true),
        None
    );
}

#[test]
fn purlis_words_are_read_as_charter_words_are() {
    charter_core::unsteered!();
    assert_eq!(
        proseguard::charter_words("purlis", &words("purlis persona remember x")),
        Some(words("persona remember x"))
    );
    assert_eq!(
        proseguard::charter_words("python3", &words("python3 -m purlis persona remember x")),
        Some(words("persona remember x"))
    );
    let spliced = "purlis persona remember devops \"$(env)\"";
    assert!(
        proseguard::charter_substitution_hit(spliced).is_some(),
        "{spliced} was let through"
    );
}

#[test]
fn a_persona_grant_of_purlis_still_asks_before_a_secret_a_vault_or_a_change() {
    charter_core::unsteered!();
    for name in ["purlis", "charter", "edm"] {
        for verb in ["secret get db", "vault add x", "change land"] {
            assert!(
                personagate::dangerous(name, &words(verb)),
                "`{name} {verb}` ran on a grant"
            );
        }
    }
}

fn attended() -> Caller<'static> {
    Caller {
        agent_id: None,
        harness: Some("claude-code"),
        permission_mode: Some("default"),
    }
}

#[test]
fn a_handoff_through_purlis_is_a_handoff() {
    charter_core::unsteered!();
    assert!(handoffguard::is_handoff("purlis handoff beta"));
    assert!(handoffguard::is_handoff("python3 -m purlis handoff beta"));
}

/// The ask rule the host prompts on is spelt `charter handoff` until the hooks written into a
/// project move to the new name (RN-7). Until then a `purlis handoff` is one that rule does not
/// match, so it is refused for its spelling — never let through without the operator's prompt.
#[test]
fn a_handoff_through_purlis_is_refused_until_the_hosts_rule_names_it() {
    charter_core::unsteered!();
    let canonical = "charter handoff beta <<'BRIEF'\nship it\nBRIEF";
    assert_eq!(handoffguard::handoff_refusal(canonical, attended()), None);
    for cmd in [
        "purlis handoff beta <<'BRIEF'\nship it\nBRIEF",
        "$'purlis' handoff beta <<'BRIEF'\nship it\nBRIEF",
        "pur''lis handoff beta <<'BRIEF'\nship it\nBRIEF",
    ] {
        assert_eq!(
            handoffguard::handoff_refusal(cmd, attended()).map(|(why, _)| why),
            Some(REASON_SPELLING),
            "{cmd}"
        );
    }
}

/// A handoff's brief is the text the new chat reads, fed to the product and never to a shell,
/// so a path it mentions is data whichever name took it.
#[test]
fn a_brief_handed_to_purlis_is_data_as_one_handed_to_charter_is() {
    charter_core::unsteered!();
    for name in ["charter", "purlis"] {
        let cmd = format!("{name} handoff beta <<'BRIEF'\ncat {}\nBRIEF", vault());
        assert_eq!(leak(&cmd), None, "{cmd}");
    }
    // …and the same body fed to a shell is still read.
    let shell = format!("bash <<'BRIEF'\ncat {}\nBRIEF", vault());
    assert!(leak(&shell).is_some(), "{shell}");
}

/// The host asks the operator before the commands its consent rules name — a report filed with
/// `--yes`, a todo promoted to a forge, a handoff — and those rules are spelt `charter …` until
/// the project is renamed (RN-7). The same command spelt `purlis …` matches none of them, so the
/// guard refuses it and says which spelling to use.
#[test]
fn a_consent_gated_command_spelt_purlis_is_refused_until_the_rules_name_it() {
    charter_core::unsteered!();
    for cmd in [
        "purlis report bug --yes 0123",
        "purlis report --yes=1 bug",
        "/usr/local/bin/purlis report bug --yes x",
        "PURLIS report bug --yes x",
        "python3 -m purlis report bug --yes x",
        "purlis ws todo promote 1",
        "purlis workspace todo promote 1 -w alpha",
        "purlis -w alpha ws todo promote 1",
        "cd /tmp && purlis ws todo promote 1",
        "purlis handoff beta",
    ] {
        let why = no_rules(cmd).unwrap_or_else(|| panic!("{cmd} was let through"));
        assert!(why.contains("`charter"), "{cmd}: {why}");
    }
}

#[test]
fn the_same_commands_spelt_charter_are_left_to_the_hosts_rule() {
    charter_core::unsteered!();
    for cmd in [
        "charter report bug --yes 0123",
        "charter ws todo promote 1",
        "charter handoff beta",
        // Not gated by any rule, under either name.
        "purlis report bug",
        "purlis ws todo list",
        "purlis status",
        "echo purlis report --yes",
    ] {
        assert_eq!(no_rules(cmd), None, "{cmd}");
    }
}

/// The refused list is read off the table the rules are written from, so a rule added there is
/// refused under the new name without a second list to remember.
#[test]
fn every_consent_rule_the_project_carries_is_refused_under_the_new_name() {
    charter_core::unsteered!();
    assert_eq!(CONSENT_PATTERNS.len(), 3);
    for pattern in CONSENT_PATTERNS {
        let spelt = pattern
            .strip_prefix("charter ")
            .expect("a rule spelt with the name it is written under")
            .replace('*', "x");
        let cmd = format!("purlis {spelt}");
        assert!(
            no_rules(&cmd).is_some(),
            "{cmd} (from `{pattern}`) was let through"
        );
    }
}

/// A heredoc a reader takes as data — a commit message, a note — is not run, so a line in it
/// that spells one of these commands is not refused; the same line fed to a shell is.
#[test]
fn a_consent_gated_command_in_a_data_heredoc_is_data_and_one_a_shell_runs_is_not() {
    charter_core::unsteered!();
    for cmd in [
        "git commit -F - <<'EOF'\npurlis report bug --yes x\nEOF",
        "cat > notes.md <<'EOF'\npurlis ws todo promote 1\nEOF",
    ] {
        assert_eq!(no_rules(cmd), None, "{cmd}");
    }
    for cmd in [
        "bash <<'EOF'\npurlis report bug --yes x\nEOF",
        "bash <<'EOF'\npurlis ws todo promote 1\nEOF",
    ] {
        assert!(no_rules(cmd).is_some(), "{cmd} was let through");
    }
}

/// One level into the string a shell is handed to run.
#[test]
fn a_consent_gated_command_in_a_shell_string_is_refused() {
    charter_core::unsteered!();
    for cmd in [
        "sh -c 'purlis report bug --yes x'",
        "bash -c \"purlis ws todo promote 1\"",
        "bash -lc 'cd /tmp && purlis report bug --yes x'",
        "eval purlis ws todo promote 1",
    ] {
        assert!(no_rules(cmd).is_some(), "{cmd} was let through");
    }
    for cmd in [
        "sh -c 'charter report bug --yes x'",
        "sh -c 'purlis report bug'",
        "echo 'purlis report bug --yes x'",
    ] {
        assert_eq!(no_rules(cmd), None, "{cmd}");
    }
}
