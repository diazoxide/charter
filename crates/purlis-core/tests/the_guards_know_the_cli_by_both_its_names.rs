//! The command line ships as `purlis`, and `charter` runs it too for the rename's window (RN-3,
//! #1255). A guard that knows the product's own binary only as `charter` would let every
//! refusal it makes be stepped around by typing the new name, so each one knows both — and still
//! knows `edm`, the name before `charter`, where it did.

use std::path::Path;

use purlis_core::handoffguard::{self, Caller};
use purlis_core::scaffold::settings::CONSENT_PATTERNS;
use purlis_core::{consentspelling, floorguard, leakguard, personagate, proseguard};

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
    consentspelling::refusal(cmd, Path::new("/nonexistent-plane"), &[])
}

fn vault() -> String {
    format!(".charter/{}/db.json", "vaults")
}

#[test]
fn purlis_is_the_product_itself_however_it_is_spelt() {
    purlis_core::unsteered!();
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
    purlis_core::unsteered!();
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
    purlis_core::unsteered!();
    for name in ["purlis", "charter"] {
        let cmd = format!("{name} secret exec devops -- cat {}", vault());
        assert!(leak(&cmd).is_some(), "{cmd} was let through");
    }
}

#[test]
fn landing_a_change_unattended_through_purlis_is_on_the_floor() {
    purlis_core::unsteered!();
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
    purlis_core::unsteered!();
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
    purlis_core::unsteered!();
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
    purlis_core::unsteered!();
    assert!(handoffguard::is_handoff("purlis handoff beta"));
    assert!(handoffguard::is_handoff("python3 -m purlis handoff beta"));
}

/// No rule of the host's consents to a handoff (#1444): it is a dispatch, and its consent is
/// purlis's own dispatch grant. So the `purlis` spelling is read as the handoff it is, in a
/// project that carries no rule for it, and is the app's to decide.
#[test]
fn a_handoff_through_purlis_is_read_as_one_and_waits_on_no_rule_of_the_hosts() {
    purlis_core::unsteered!();
    for cmd in [
        "charter handoff beta <<'BRIEF'\nship it\nBRIEF",
        "purlis handoff beta <<'BRIEF'\nship it\nBRIEF",
        "$'purlis' handoff beta <<'BRIEF'\nship it\nBRIEF",
        "pur''lis handoff beta <<'BRIEF'\nship it\nBRIEF",
        "/usr/local/bin/purlis handoff beta <<'BRIEF'\nship it\nBRIEF",
    ] {
        assert_eq!(
            handoffguard::handoff_refusal(cmd, attended()),
            None,
            "{cmd}"
        );
        assert_eq!(no_rules(cmd), None, "{cmd}");
    }
    // A helper sub-agent's is refused under either name, as it was.
    let helper = Caller {
        agent_id: Some("sub-1"),
        ..attended()
    };
    for name in ["charter", "purlis"] {
        let cmd = format!("{name} handoff beta <<'BRIEF'\nship it\nBRIEF");
        assert_eq!(
            handoffguard::handoff_refusal(&cmd, helper).map(|(why, _)| why),
            Some(handoffguard::REASON_SUBAGENT),
            "{cmd}"
        );
    }
}

/// Where a handoff sits is still read: inside a substitution, a string or a heredoc a shell
/// runs, purlis cannot read its brief or tell whose it is, under either name.
#[test]
fn a_handoff_where_purlis_cannot_read_it_is_refused_under_either_name() {
    purlis_core::unsteered!();
    for name in ["charter", "purlis"] {
        for cmd in [
            format!("echo \"$({name} handoff beta)\""),
            format!("x=`{name} handoff beta`"),
        ] {
            let why = no_rules(&cmd).unwrap_or_else(|| panic!("{cmd} was let through"));
            assert_eq!(
                why,
                handoffguard::handoff_placed("inside a command substitution"),
                "{cmd}"
            );
        }
        for cmd in [
            format!("sh -c '{name} handoff beta'"),
            format!("bash <<'EOF'\n{name} handoff beta\nEOF"),
        ] {
            assert_eq!(
                handoffguard::handoff_refusal(&cmd, attended()).map(|(why, _)| why),
                Some(handoffguard::REASON_SHELL_STRING),
                "{cmd}"
            );
        }
    }
}

/// A handoff's brief is the text the new chat reads, fed to the product and never to a shell,
/// so a path it mentions is data whichever name took it.
#[test]
fn a_brief_handed_to_purlis_is_data_as_one_handed_to_charter_is() {
    purlis_core::unsteered!();
    for name in ["charter", "purlis"] {
        let cmd = format!("{name} handoff beta <<'BRIEF'\ncat {}\nBRIEF", vault());
        assert_eq!(leak(&cmd), None, "{cmd}");
    }
    // …and the same body fed to a shell is still read.
    let shell = format!("bash <<'BRIEF'\ncat {}\nBRIEF", vault());
    assert!(leak(&shell).is_some(), "{shell}");
}

/// The host asks the operator before the commands its consent rules name — a report filed with
/// `--yes`, a todo promoted to a forge — and those rules are spelt `charter …` until
/// the project is renamed (RN-7). The same command spelt `purlis …` matches none of them, so the
/// guard refuses it and says which spelling to use.
#[test]
fn a_consent_gated_command_spelt_purlis_is_refused_until_the_rules_name_it() {
    purlis_core::unsteered!();
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
        "echo \"$(purlis report bug --yes x)\"",
    ] {
        let why = no_rules(cmd).unwrap_or_else(|| panic!("{cmd} was let through"));
        assert!(why.contains("`charter"), "{cmd}: {why}");
    }
}

#[test]
fn the_same_commands_spelt_charter_are_left_to_the_hosts_rule() {
    purlis_core::unsteered!();
    for cmd in [
        "charter report bug --yes 0123",
        "charter ws todo promote 1",
        "charter handoff beta",
        // Not gated by any rule, under either name.
        "purlis handoff beta",
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
    purlis_core::unsteered!();
    assert_eq!(CONSENT_PATTERNS.len(), 2);
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
    purlis_core::unsteered!();
    for cmd in [
        "git commit -F - <<'EOF'\npurlis report bug --yes x\nEOF",
        "cat > notes.md <<'EOF'\npurlis ws todo promote 1\nEOF",
        "gh pr create --title t --body \"$(cat <<'EOF'\npurlis report bug --yes x\n`purlis ws todo promote 1`\nEOF\n)\"",
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
    purlis_core::unsteered!();
    for cmd in [
        "sh -c 'purlis report bug --yes x'",
        "bash -c \"purlis ws todo promote 1\"",
        "bash -lc 'cd /tmp && purlis report bug --yes x'",
        "eval purlis ws todo promote 1",
    ] {
        assert!(no_rules(cmd).is_some(), "{cmd} was let through");
    }
    for cmd in [
        "sh -c 'purlis report bug'",
        "echo 'purlis report bug --yes x'",
    ] {
        assert_eq!(no_rules(cmd), None, "{cmd}");
    }
}

/// Every guard reads a program's name as the filesystem does, ignoring case: on one that folds
/// case `CHARTER` runs the same binary, and a persona's grant must not run its secrets, its
/// vaults or its changes without a prompt because of a Shift key.
#[test]
fn the_persona_gate_reads_the_programs_name_in_any_case() {
    purlis_core::unsteered!();
    for name in ["PURLIS", "Charter", "EDM"] {
        for verb in ["secret get db", "vault add x", "change land"] {
            assert!(
                personagate::dangerous(name, &words(verb)),
                "`{name} {verb}` ran on a grant"
            );
        }
    }
    assert!(personagate::dangerous("Kubectl", &words("delete pod x")));
    assert!(personagate::dangerous(
        "GH",
        &words("release delete-asset v1 a")
    ));
    assert!(!personagate::dangerous("PURLIS", &words("status")));
}

/// The host asks before `charter report … --yes` and `charter … todo … promote` only when the
/// command is spelt the way its rule is: the bare name at the start of its command. Any other
/// spelling the guard reads as the same command — a path, another case, a prefix, quoting, a
/// subshell — is one the rule does not match, so the guard refuses it and names the spelling
/// that asks.
#[test]
fn a_consent_gated_command_spelt_charter_any_other_way_is_refused() {
    purlis_core::unsteered!();
    for cmd in [
        "/usr/local/bin/charter report bug --yes x",
        "./charter ws todo promote 1",
        "~/.local/bin/charter report bug --yes x",
        "/Applications/purlis.app/Contents/MacOS/charter ws todo promote 1",
        "CHARTER report bug --yes x",
        "python3 -m charter report bug --yes x",
        "FOO=1 charter report bug --yes x",
        "env charter ws todo promote 1",
        "timeout 60 charter report bug --yes x",
        "\\charter report bug --yes x",
        "'charter' ws todo promote 1",
        "charter 'report' bug --yes x",
        "charter report bug --y\\es x",
        "(charter report bug --yes x)",
        "echo $(charter ws todo promote 1)",
        "echo \"`charter report bug --yes x`\"",
        "echo \"$(charter ws todo promote 1)\"",
        "cd /tmp && /usr/local/bin/charter ws todo promote 1",
    ] {
        let why = no_rules(cmd).unwrap_or_else(|| panic!("{cmd} was let through"));
        assert!(why.contains("`charter "), "{cmd}: {why}");
    }
}

/// …and the spelling the rule matches, every command no rule gates and every mention of one in
/// data stay the host's to judge, so the backstop costs nothing where a prompt is already shown.
#[test]
fn the_spelling_the_hosts_rule_matches_and_mere_mentions_are_left_alone() {
    purlis_core::unsteered!();
    for cmd in [
        "charter report bug --yes x",
        "charter report --yes=1 bug",
        "charter -w alpha ws todo promote 1",
        "cd /tmp && charter ws todo promote 1",
        "charter report bug --yes x 2>&1 | tail -5",
        "/usr/local/bin/charter report bug",
        "./charter ws todo list",
        "CHARTER status",
        "echo charter report bug --yes x",
        "echo '/usr/local/bin/charter report bug --yes x'",
        "echo \"./charter ws todo promote 1\"",
        "git commit -m \"$(cat msg.txt)\" -m 'then CHARTER report bug --yes x'",
        "grep -rn 'charter ws todo promote' docs",
        "git commit -m 'run ./charter report bug --yes x to file it'",
        "git commit -F - <<'EOF'\n/usr/local/bin/charter report bug --yes x\nEOF",
        "cat > notes.md <<'EOF'\n./charter ws todo promote 1\nEOF",
        "git commit -m \"$(cat <<'EOF'\nDocs: run /usr/local/bin/charter report bug --yes x\nEOF\n)\"",
        "gh pr create --title t --body \"$(cat <<'EOF'\n`./charter ws todo promote 1` asks first\nEOF\n)\"",
        "git commit -m 'docs: `CHARTER report bug --yes x` is refused'",
        // A handoff is A7's to judge, which reads its spelling more closely than this does.
        "/usr/local/bin/charter handoff beta",
        "python3 -m charter handoff beta",
    ] {
        assert_eq!(no_rules(cmd), None, "{cmd}");
    }
}

/// The host's rule reads the outer command, so a report or a promote spelt `charter …` inside a
/// string or a heredoc a shell runs gets no prompt. The guard reads one level in, as A7 does
/// for a handoff and as A7b does for the new name.
#[test]
fn a_consent_gated_command_spelt_charter_inside_a_shell_string_is_refused() {
    purlis_core::unsteered!();
    for cmd in [
        "sh -c 'charter report bug --yes x'",
        "bash -c \"charter ws todo promote 1\"",
        "bash -lc 'cd /tmp && charter report bug --yes x'",
        "eval charter ws todo promote 1",
        "eval 'charter report bug --yes x'",
        "bash <<'EOF'\ncharter report bug --yes x\nEOF",
        "cat <<'EOF' | sh\ncharter ws todo promote 1\nEOF",
    ] {
        let why = no_rules(cmd).unwrap_or_else(|| panic!("{cmd} was let through"));
        assert!(why.contains("`charter "), "{cmd}: {why}");
    }
    for cmd in [
        "sh -c 'charter report bug'",
        "sh -c 'echo charter report bug --yes x'",
        "bash -c 'grep \"charter ws todo promote\" docs'",
        "bash <<'EOF'\ncharter status\nEOF",
    ] {
        assert_eq!(no_rules(cmd), None, "{cmd}");
    }
}
