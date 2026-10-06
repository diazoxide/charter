//! **An operator's own ask and deny rules read every spelling of the program they name** (#1286).
//!
//! A host rule (`purlis guard ask 'terraform apply *'`) is a glob over the command as written,
//! so a path to the binary, another case, a `VAR=` prefix or a wrapper, a quoted or escaped
//! word, a subshell or a substitution, or a string a shell runs is a spelling it does not match.
//! #1279 closed that for the project's own consent rules; this is the same backstop, read the
//! same way, for every Bash rule the operator wrote. The guard refuses a spelling the rule does
//! not match and names the one it does; the spelling the rule matches, and every mention of the
//! program in data, are left to the host.

use purlis_core::rulespelling::{self, Rules};

/// What the operator wrote in `.claude/settings.json`: ask before `terraform apply`, before the
/// command line's `secret` under its old name and before any `kubectl delete`; deny `rm -rf`.
const SETTINGS: &str = r#"{"permissions": {
    "allow": ["Bash(git status)"],
    "ask": ["Bash(terraform apply *)", "Bash(charter secret *)", "Bash(*kubectl delete*)"],
    "deny": ["Bash(rm -rf *)"]}}"#;

/// …and in `opencode.json`.
const OPENCODE: &str = r#"{"permission": {"bash": {"terraform apply *": "ask",
    "charter secret *": "ask", "rm -rf *": "deny"}}}"#;

fn project() -> Rules {
    Rules::of_claude_settings(".claude/settings.json", SETTINGS).and(Rules::of_opencode(OPENCODE))
}

fn judged(rules: &Rules, cmd: &str) -> Option<String> {
    rules.refusal(cmd)
}

#[test]
fn a_program_an_operators_rule_names_spelt_another_way_is_refused() {
    purlis_core::unsteered!();
    let dir = project();
    for cmd in [
        "/usr/local/bin/terraform apply -auto-approve",
        "./terraform apply",
        "~/bin/terraform apply x",
        "TERRAFORM apply x",
        "Terraform apply x",
        "FOO=1 terraform apply x",
        "env terraform apply x",
        "env -i terraform apply x",
        "timeout 60 terraform apply x",
        "sudo terraform apply x",
        "nohup terraform apply x",
        "command terraform apply x",
        "exec terraform apply x",
        "xargs terraform apply",
        "time terraform apply x",
        "terra\\\nform apply x",
        "terraform app\\ly x",
        "echo \"`terraform apply x`\"",
        "\\terraform apply x",
        "'terraform' apply x",
        "\"terraform\" apply x",
        "terra''form apply x",
        "$'terraform' apply x",
        "terraform 'apply' x",
        "terraform  apply x",
        "terraform\tapply x",
        "(terraform apply x)",
        "{ terraform apply x; }",
        "if true; then terraform apply x; fi",
        "! terraform apply x",
        "echo $(terraform apply x)",
        "echo `terraform apply x`",
        "echo \"$(terraform apply x)\"",
        "cd /tmp && /usr/local/bin/terraform apply x",
        "/bin/rm -rf build",
        "'rm' -rf build",
    ] {
        let why = judged(&dir, cmd).unwrap_or_else(|| panic!("{cmd} was let through"));
        assert!(why.contains("rule"), "{cmd}: {why}");
    }
}

#[test]
fn the_refusal_names_the_rule_and_where_it_is_written() {
    purlis_core::unsteered!();
    let dir = project();
    let why = judged(&dir, "/usr/local/bin/terraform apply").expect("refused");
    assert!(why.contains("`terraform apply *`"), "{why}");
    assert!(why.contains(".claude/settings.json"), "{why}");
    let deny = judged(&dir, "/bin/rm -rf build").expect("refused");
    assert!(
        deny.contains("`rm -rf *`") && deny.contains("den"),
        "{deny}"
    );
}

#[test]
fn a_rule_under_either_name_of_the_command_line_holds_under_the_other() {
    purlis_core::unsteered!();
    let dir = project();
    for cmd in [
        "purlis secret get db password",
        "/usr/local/bin/purlis secret get db password",
        "python3 -m purlis secret get db password",
        "/usr/local/bin/charter secret get db password",
        "CHARTER secret get db password",
    ] {
        assert!(judged(&dir, cmd).is_some(), "{cmd} was let through");
    }
    assert_eq!(judged(&dir, "charter secret get db password"), None);

    // The project carries the twin: the new name, spelt as that twin is, is the host's to ask.
    let dir = Rules::of_claude_settings(
        ".claude/settings.json",
        r#"{"permissions": {"ask": ["Bash(charter secret *)", "Bash(purlis secret *)"]}}"#,
    )
    .and(Rules::of_opencode(
        r#"{"permission": {"bash": {"charter secret *": "ask", "purlis secret *": "ask"}}}"#,
    ));
    assert_eq!(judged(&dir, "purlis secret get db password"), None);
    assert!(judged(&dir, "/usr/local/bin/purlis secret get db password").is_some());
}

#[test]
fn a_ruled_command_inside_a_string_or_heredoc_a_shell_runs_is_refused() {
    purlis_core::unsteered!();
    let dir = project();
    for cmd in [
        "sh -c 'terraform apply x'",
        "bash -c \"terraform apply x\"",
        "bash -lc 'cd /tmp && terraform apply x'",
        "eval terraform apply x",
        "eval 'rm -rf build'",
        "xargs sh -c 'terraform apply x'",
        "bash <<'EOF'\nterraform apply x\nEOF",
        "cat <<'EOF' | sh\nterraform apply x\nEOF",
    ] {
        let why = judged(&dir, cmd).unwrap_or_else(|| panic!("{cmd} was let through"));
        assert!(why.contains("shell"), "{cmd}: {why}");
    }
}

#[test]
fn the_spelling_the_rule_matches_and_mere_mentions_are_left_to_the_host() {
    purlis_core::unsteered!();
    let dir = project();
    for cmd in [
        "terraform apply x",
        "terraform apply",
        "cd /tmp && terraform apply x",
        "terraform apply x 2>&1 | tail -5",
        "rm -rf build",
        "/usr/local/bin/terraform plan",
        "TERRAFORM plan",
        "rm build",
        "echo terraform apply x",
        "echo '/usr/local/bin/terraform apply x'",
        "echo \"TERRAFORM apply x\"",
        "grep -rn 'terraform apply' docs",
        "git commit -m 'run /usr/local/bin/terraform apply to ship it'",
        "git commit -F - <<'EOF'\n/usr/local/bin/terraform apply x\nEOF",
        "cat > notes.md <<'EOF'\n'rm' -rf build\nEOF",
        "git commit -m \"$(cat <<'EOF'\nDocs: run /usr/local/bin/terraform apply\nEOF\n)\"",
        "gh pr create --title t --body \"$(cat <<'EOF'\n`TERRAFORM apply x` asks first\nEOF\n)\"",
        "sh -c 'terraform plan'",
        "sh -c 'echo terraform apply x'",
        "bash <<'EOF'\nterraform plan\nEOF",
        // A rule whose program is a wildcard names no program to read every spelling of.
        "'kubectl' delete pod x",
        // An allow rule is not one the guard stands behind.
        "/usr/bin/git status",
    ] {
        assert_eq!(judged(&dir, cmd), None, "{cmd}");
    }
}

/// A rule in one harness's file and not the other's still holds; a twin in another file does
/// not stand in for it; and a project with no rules of the operator's costs nothing.
#[test]
fn a_rule_in_any_one_file_holds_and_a_project_without_rules_is_left_alone() {
    purlis_core::unsteered!();
    assert_eq!(
        Rules::default().refusal("/usr/local/bin/terraform apply"),
        None
    );
    let one = Rules::of_opencode(r#"{"permission": {"bash": {"terraform apply *": "ask"}}}"#);
    assert!(one.refusal("/usr/local/bin/terraform apply").is_some());
    assert_eq!(one.refusal("terraform apply"), None);

    let elsewhere = one.and(Rules::of_claude_settings(
        ".claude/settings.json",
        r#"{"permissions": {"ask": ["Bash(/usr/local/bin/terraform apply *)"]}}"#,
    ));
    assert!(
        elsewhere
            .refusal("/usr/local/bin/terraform apply x")
            .is_some()
    );

    // An ask rule the source matches does not stand in for a deny.
    let weaker = Rules::of_claude_settings(
        ".claude/settings.json",
        r#"{"permissions": {"deny": ["Bash(rm -rf *)"], "ask": ["Bash(/bin/rm *)"]}}"#,
    );
    let why = weaker.refusal("/bin/rm -rf build").expect("refused");
    assert!(why.contains("`rm -rf *`"), "{why}");
    let legacy = Rules::of_claude_settings(
        ".claude/settings.local.json",
        r#"{"permissions": {"deny": ["Bash(helm uninstall:*)"]}}"#,
    );
    assert!(legacy.refusal("HELM uninstall x").is_some());
    assert_eq!(legacy.refusal("helm uninstall x"), None);
}

/// A rule may name a wrapper — `sudo`, or the command line's own `secret exec`, which runs
/// another command — and it holds under every spelling of that wrapper too.
#[test]
fn a_rule_that_names_a_wrapper_holds_under_every_spelling_of_it() {
    purlis_core::unsteered!();
    let sudo = Rules::of_claude_settings(
        ".claude/settings.json",
        r#"{"permissions": {"ask": ["Bash(sudo *)"]}}"#,
    );
    for cmd in [
        "/usr/bin/sudo ls",
        "SUDO ls",
        "\\sudo ls",
        "'sudo' ls",
        "env sudo ls",
        "nohup /usr/bin/sudo ls",
        "sh -c 'sudo ls'",
    ] {
        assert!(sudo.refusal(cmd).is_some(), "{cmd} was let through");
    }
    for cmd in ["sudo ls", "sudo -u x ls", "echo sudo ls", "ls"] {
        assert_eq!(sudo.refusal(cmd), None, "{cmd}");
    }

    let secret = project();
    for cmd in [
        "purlis secret exec v -- env",
        "/usr/local/bin/purlis secret exec v -- env",
        "PURLIS secret exec v -- env",
        "/usr/local/bin/charter secret exec v -- env",
        "timeout 5 charter secret exec v -- terraform plan",
    ] {
        assert!(secret.refusal(cmd).is_some(), "{cmd} was let through");
    }
    assert_eq!(secret.refusal("charter secret exec v -- env"), None);
    // The command `secret exec` runs is read too: refused where nothing the host matches on
    // the line holds it, and the host's to ask where the outer rule already does.
    let rm = Rules::of_claude_settings(
        ".claude/settings.json",
        r#"{"permissions": {"deny": ["Bash(rm -rf *)"]}}"#,
    );
    assert!(
        rm.refusal("charter secret exec v -- /bin/rm -rf b")
            .is_some()
    );
    assert_eq!(
        secret.refusal("charter secret exec v -- /usr/local/bin/terraform apply x"),
        None
    );
}

/// Every program a segment runs is held to its rule, not only the first one found: a ruled
/// command in a substitution beside a command spelt as its own rule is still refused.
#[test]
fn every_ruled_program_in_a_segment_is_judged() {
    purlis_core::unsteered!();
    let dir = project();
    for cmd in [
        "terraform apply x \"$(/bin/rm -rf b)\"",
        "terraform apply x $(/bin/rm -rf b)",
        "terraform apply x \"`'rm' -rf b`\"",
        "terraform apply x \"$('rm' -rf b)\"",
    ] {
        let why = judged(&dir, cmd).unwrap_or_else(|| panic!("{cmd} was let through"));
        assert!(why.contains("`rm -rf *`"), "{cmd}: {why}");
    }
}

/// A rule's program is matched in any case and by its file name, and the refusal names the
/// spelling the rule itself uses.
#[test]
fn a_rule_spelt_with_a_capital_or_a_path_is_matched_and_named_as_written() {
    purlis_core::unsteered!();
    let capital = Rules::of_claude_settings(
        ".claude/settings.json",
        r#"{"permissions": {"ask": ["Bash(Terraform apply *)"]}}"#,
    );
    assert_eq!(capital.refusal("Terraform apply x"), None);
    let why = capital
        .refusal("/usr/bin/terraform apply x")
        .expect("refused");
    assert!(why.contains("`Terraform`"), "{why}");

    let path = Rules::of_claude_settings(
        ".claude/settings.json",
        r#"{"permissions": {"ask": ["Bash(/usr/local/bin/terraform apply *)"]}}"#,
    );
    assert_eq!(path.refusal("/usr/local/bin/terraform apply x"), None);
    let why = path.refusal("terraform apply x").expect("refused");
    assert!(why.contains("`/usr/local/bin/terraform`"), "{why}");
    assert!(!why.contains("bare name"), "{why}");
}

/// A rules file that is a FIFO or far too large is not read, so it cannot hang or stall the
/// guard on a tool call.
#[cfg(unix)]
#[test]
fn a_rules_file_that_would_hang_or_stall_the_guard_is_not_read() {
    purlis_core::unsteered!();
    let dir = tempfile::tempdir().expect("a directory");
    std::fs::create_dir_all(dir.path().join(".claude")).expect(".claude");
    let made = purlis_core::forklock::status(
        std::process::Command::new("mkfifo").arg(dir.path().join(".claude/settings.json")),
    )
    .expect("mkfifo runs");
    assert!(made.success());
    let big = std::fs::File::create(dir.path().join(".claude/settings.local.json")).expect("big");
    big.set_len(2 * 1024 * 1024).expect("sized");
    assert_eq!(
        rulespelling::refusal("/usr/local/bin/terraform apply", dir.path(), &[dir.path()]),
        None
    );
}

/// A settings file kept elsewhere and linked into place — a dotfiles manager does this — is
/// read through the link, as the host reads it.
#[cfg(unix)]
#[test]
fn a_linked_settings_file_is_read_through_the_link() {
    purlis_core::unsteered!();
    let dir = tempfile::tempdir().expect("a directory");
    let kept = dir.path().join("dotfiles-settings.json");
    std::fs::write(
        &kept,
        r#"{"permissions": {"ask": ["Bash(terraform apply *)"]}}"#,
    )
    .expect("the kept file");
    std::fs::create_dir_all(dir.path().join(".claude")).expect(".claude");
    std::os::unix::fs::symlink(&kept, dir.path().join(".claude/settings.json")).expect("link");
    assert!(rulespelling::refusal("/usr/local/bin/terraform apply", dir.path(), &[]).is_some());
    assert_eq!(
        rulespelling::refusal("terraform apply", dir.path(), &[]),
        None
    );
}

/// The files the host may read are found on disk: a workspace's generated layer, where the
/// session runs, carries its own copy of the rules, and a machine-local file the operator wrote
/// with `--local` is read too.
#[test]
fn a_rule_in_a_layer_or_a_machine_local_file_holds() {
    purlis_core::unsteered!();
    let dir = tempfile::tempdir().expect("a directory");
    let ws = dir.path().join("workspaces/alpha");
    std::fs::create_dir_all(ws.join(".claude")).expect("layer");
    std::fs::write(
        ws.join(".claude/settings.json"),
        r#"{"permissions": {"ask": ["Bash(terraform apply *)"]}}"#,
    )
    .expect("layer settings");
    assert!(rulespelling::refusal("/usr/local/bin/terraform apply", dir.path(), &[&ws]).is_some());
    assert_eq!(
        rulespelling::refusal("/usr/local/bin/terraform apply", dir.path(), &[]),
        None
    );

    std::fs::create_dir_all(dir.path().join(".claude")).expect(".claude");
    std::fs::write(
        dir.path().join(".claude/settings.local.json"),
        r#"{"permissions": {"deny": ["Bash(helm uninstall *)"]}}"#,
    )
    .expect("local settings");
    let why = rulespelling::refusal("HELM uninstall x", dir.path(), &[]).expect("refused");
    assert!(why.contains(".claude/settings.local.json"), "{why}");
    assert_eq!(
        rulespelling::refusal("helm uninstall x", dir.path(), &[]),
        None
    );
}

/// A command whose program stands behind more wrappers than the guard reads through is
/// refused, saying why, rather than read part of the way.
#[test]
fn a_program_behind_too_many_wrappers_is_refused() {
    purlis_core::unsteered!();
    let dir = project();
    let deep = format!("{}terraform apply x", "env ".repeat(65));
    let why = judged(&dir, &deep).expect("refused");
    assert!(why.contains("wrappers"), "{why}");
    let shallow = format!("{}terraform apply x", "env ".repeat(60));
    assert!(
        judged(&dir, &shallow).is_some(),
        "an env prefix is still not the rule's spelling"
    );
    assert_eq!(judged(&dir, &format!("{}ls", "nice ".repeat(60))), None);
}

/// The guards' time grows with the command's length, never faster: a crafted line must not run
/// a hook past the harness's deadline, which would let the call through (#1286). Four times
/// the input may take a little over four times as long; a reading that grew with its square
/// would take sixteen.
#[test]
fn the_guards_take_time_in_proportion_to_the_command() {
    purlis_core::unsteered!();
    use std::path::Path;
    use std::time::{Duration, Instant};
    let rules = Rules::of_claude_settings(
        ".claude/settings.json",
        r#"{"permissions": {"ask": ["Bash(terraform apply *)", "Bash(sudo *)"]}}"#,
    );
    let guards = |cmd: &str| {
        let started = Instant::now();
        let _ = purlis_core::consentspelling::refusal(cmd, Path::new("/nonexistent-plane"), &[]);
        let _ = rules.refusal(cmd);
        let _ = purlis_core::leakguard::leak_reason(cmd, "", Path::new("/nonexistent-state"));
        started.elapsed()
    };
    type Line = fn(usize) -> String;
    let shapes: [(&str, Line); 5] = [
        ("a chain of wrappers", |n| format!("{}ls", "env ".repeat(n))),
        ("unclosed substitutions", |n| {
            format!("{}ls", "echo \"$(".repeat(n))
        }),
        ("closed substitutions", |n| {
            format!("{}ls{}", "echo \"$(".repeat(n), ")\"".repeat(n))
        }),
        ("nested parameter expansions", |n| {
            format!("echo \"$({}ls)\"", "${x:-".repeat(n))
        }),
        ("a message with parens in a heredoc", |n| {
            format!(
                "git commit -m \"$(cat <<'EOF'\n{}EOF\n)\"",
                "1) it's a line\n".repeat(n)
            )
        }),
    ];
    for (shape, line) in shapes {
        // The faster of two runs each, so a moment of load on the machine is not the measure.
        let time = |n: usize| guards(&line(n)).min(guards(&line(n)));
        let small = time(2000);
        let large = time(8000);
        assert!(
            large < small * 10 + Duration::from_millis(200),
            "{shape}: 2000 took {small:?}, 8000 took {large:?}"
        );
    }
}

/// A commit or pull request message written through `"$(cat <<'EOF' … EOF)"` is data to every
/// shell even when it holds a `)`, so a line in it that starts with a ruled program is not
/// refused.
#[test]
fn a_message_with_a_paren_in_a_quoted_heredoc_is_data() {
    purlis_core::unsteered!();
    let rules = Rules::of_claude_settings(
        ".claude/settings.json",
        r#"{"permissions": {"ask": ["Bash(git push *)", "Bash(terraform apply *)",
            "Bash(sudo *)"]}}"#,
    );
    for cmd in [
        "git commit -m \"$(cat <<'EOF'\n1) Build first\nGit push now waits for CI\nEOF\n)\"",
        "git commit -m \"$(cat <<'EOF'\nFix the build (see #12)\ngit push now waits for CI\nEOF\n)\"",
        "git commit -m \"$(cat <<'EOF'\n- a) first\nTerraform apply asks first now\nEOF\n)\"",
        "gh pr create --title t --body \"$(cat <<'EOF'\n1) Summary\nSudo prompts are kept\nEOF\n)\"",
        "git commit -m \"$(cat <<'EOF'\n1) build\n/usr/bin/terraform apply x\nEOF\n)\"",
    ] {
        assert_eq!(rules.refusal(cmd), None, "{cmd}");
    }
    // Unquoted, GNU bash 3.2 ends the substitution at the `)` and runs the next line; and in
    // double quotes, a quote, a backtick or a `$(` after it runs there too.
    for cmd in [
        "x=$(cat <<'EOF'\n1) build\n/usr/bin/terraform apply x\nEOF\n)",
        "git commit -m \"$(cat <<'EOF'\n1) build \"\n/usr/bin/terraform apply x\nEOF\n)\"",
    ] {
        assert!(rules.refusal(cmd).is_some(), "{cmd} was let through");
    }
}

/// A heredoc body or a `${ … }` inside a substitution does not end it early, so the command in
/// the substitution after it is read.
#[test]
fn a_heredoc_or_a_parameter_expansion_does_not_hide_the_next_substitution() {
    purlis_core::unsteered!();
    let dir = project();
    for cmd in [
        "echo \"$(cat <<'EOF'\nit's\nEOF\n)\" \"$(/usr/bin/terraform apply)\"",
        "echo \"$(echo ${x:-)}; /usr/bin/terraform apply)\"",
    ] {
        assert!(judged(&dir, cmd).is_some(), "{cmd} was let through");
    }
}
