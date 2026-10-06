//! What a plane's harness profiles are, and why each refused one is refused.
//!
//! Every expectation here was taken FROM the Python charter by running it over the same
//! `charter.local.toml` these tests write (`charter/profiles.py`, ADR 0022). Two of them
//! refuted what this file first claimed, which is the reason they are taken that way: the
//! registry's order is `claude, opencode, codex` and not alphabetical, and a typo'd `env`
//! key raises TWO refusals rather than one.
//!
//! A profile's command runs on a click, with no prompt between the click and the exec. That
//! is why a refusal here is a refusal and not a warning.

use std::fs;
use std::path::Path;

use purlis_core::profiles::{self, Source};

mod support;

/// A plane with the two files profiles are read from.
fn plane(committed: &str, local: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("charter.toml"), committed).unwrap();
    if !local.is_empty() {
        fs::write(dir.path().join(profiles::LOCAL_FILE), local).unwrap();
    }
    dir
}

fn why(set: &profiles::ProfileSet, name: &str) -> String {
    set.refused
        .iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("nothing refused '{name}'; refused: {:?}", set.refused))
        .reason
        .clone()
}

fn names(set: &profiles::ProfileSet) -> Vec<&str> {
    set.profiles().iter().map(|p| p.name.as_str()).collect()
}

#[test]
fn a_plane_that_declares_nothing_still_has_one_profile_per_harness_charter_knows() {
    purlis_core::unsteered!();
    // `charter/profiles.py:218` — a built-in per registered kind, so a plane that has no
    // `charter.local.toml` sees no change. In REGISTRY order, which the oracle says is
    // claude, opencode, codex; not alphabetical, and the listing prints it.
    let dir = plane("[plane]\nname = \"p\"\n", "");

    let set = profiles::derive(dir.path());

    assert_eq!(names(&set), ["claude", "opencode", "codex"]);
    assert!(set.profiles().iter().all(|p| p.source == Source::BuiltIn));
    assert_eq!(set.refused, []);
}

#[test]
fn a_profile_in_the_committed_file_is_refused_with_a_pointer_to_the_local_one() {
    purlis_core::unsteered!();
    // ADR 0022's central rule. A command in the committed file could be changed by a merged
    // pull request and then run on every machine that pulls it.
    let dir = plane(
        "[harness.committed-one]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
        "",
    );

    let set = profiles::derive(dir.path());

    assert_eq!(
        why(&set, "committed-one"),
        "[harness.committed-one] is in charter.toml, which is committed — a profile's \
         command runs on a click, so purlis reads profiles only from charter.local.toml, \
         which stays on this machine. Move the table there; charter.toml's [harness] keeps \
         `default` alone."
    );
    assert_eq!(names(&set), ["claude", "opencode", "codex"]);
}

#[test]
fn a_declared_profile_replaces_the_built_in_of_its_name_in_that_built_ins_place() {
    purlis_core::unsteered!();
    // The operator said how `claude` runs on this machine. Its POSITION is the built-in's,
    // because the listing is ordered and a replacement is not a new row at the end.
    let dir = plane(
        "",
        "[harness.claude]\nkind = \"claude\"\ncommand = [\"~/.local/bin/claude\"]\n\
         [harness.work]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
    );

    let set = profiles::derive(dir.path());

    assert_eq!(names(&set), ["claude", "opencode", "codex", "work"]);
    let claude = set.get("claude").unwrap();
    assert_eq!(claude.command, ["~/.local/bin/claude"]);
    assert_eq!(claude.source, Source::Local);
}

#[test]
fn a_refused_replacement_takes_the_built_in_name_down_with_it() {
    purlis_core::unsteered!();
    // Ruling 37: the operator said how that name runs, and the built-in standing in would
    // run the command they replaced — review 13's `enviroment` typo launching the default
    // account is exactly that case.
    let dir = plane(
        "",
        "[harness.claude]\nkind = \"claude\"\ncommand = [\"claude\"]\n\
         enviroment = { CLAUDE_CONFIG_DIR = \"~/.x\" }\n",
    );

    let set = profiles::derive(dir.path());

    assert_eq!(names(&set), ["opencode", "codex"]);
    assert!(set.get("claude").is_none());
}

#[test]
fn a_key_a_profile_does_not_have_refuses_that_profile_rather_than_being_ignored() {
    purlis_core::unsteered!();
    // Review 13. A typo such as `enviroment` would otherwise DROP `CLAUDE_CONFIG_DIR` and
    // launch the default account without a word — the failure the feature exists to prevent,
    // arrived at by spelling. The oracle raises two refusals for this one table, because
    // once parsed a typo'd key holding an inline table and a dotted name cannot be told
    // apart, so both readings are named.
    let dir = plane(
        "",
        "[harness.typo-env]\nkind = \"claude\"\ncommand = [\"claude\"]\n\
         enviroment = { CLAUDE_CONFIG_DIR = \"~/.x\" }\n",
    );

    let set = profiles::derive(dir.path());

    assert_eq!(
        why(&set, "typo-env"),
        "profile 'typo-env' has enviroment, which purlis does not read — a profile is \
         kind, command and env. Remove it."
    );
    assert_eq!(
        why(&set, "typo-env.enviroment"),
        "[harness.typo-env] holds a table enviroment, which purlis reads neither way — \
         enviroment is not a key a profile has (kind, command and env), and if a profile \
         named 'typo-env.enviroment' was meant, a profile's name is letters, digits, '_' \
         and '-', with no dot — the plane format fixes that alphabet. Rename the key, or \
         give that profile a name of its own."
    );
}

#[test]
fn a_parent_holding_only_sub_tables_declares_nothing_and_leaves_its_built_in_alone() {
    purlis_core::unsteered!();
    // F4: `[harness.claude.alt]` alone is a dotted name written without quotes. The parent
    // is a declaration only when it carries keys of its own.
    let dir = plane(
        "",
        "[harness.claude.alt]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
    );

    let set = profiles::derive(dir.path());

    assert_eq!(names(&set), ["claude", "opencode", "codex"]);
    assert_eq!(set.get("claude").unwrap().source, Source::BuiltIn);
    assert!(why(&set, "claude.alt").starts_with("[harness.claude] holds a table alt,"));
}

#[test]
fn a_kind_charter_cannot_launch_refuses_the_profile_and_names_every_kind_it_can() {
    purlis_core::unsteered!();
    let dir = plane("", "[harness.x]\nkind = \"opencodex\"\ncommand = [\"x\"]\n");

    let set = profiles::derive(dir.path());

    assert_eq!(
        why(&set, "x"),
        "profile 'x' has kind opencodex, which is not a harness purlis can launch — one \
         of: claude, opencode, codex. Set kind to one of them."
    );
}

#[test]
fn a_command_written_as_a_shell_string_is_refused_because_no_shell_runs_it() {
    purlis_core::unsteered!();
    let dir = plane(
        "",
        "[harness.x]\nkind = \"codex\"\ncommand = \"codex resume\"\n",
    );

    let set = profiles::derive(dir.path());

    assert_eq!(
        why(&set, "x"),
        "profile 'x' has no usable command — command is a list of arguments, [\"claude\"], \
         never a shell string, because no shell runs it. Write it as a list."
    );
}

#[test]
fn an_env_name_that_looks_like_a_credential_is_refused_and_points_at_the_harnesss_own_login() {
    purlis_core::unsteered!();
    // Measured on Claude Code and Codex: a variable set on the harness process reaches the
    // shell the model runs. Charter declines to hold a credential in a profile.
    let dir = plane(
        "",
        "[harness.x]\nkind = \"claude\"\ncommand = [\"claude\"]\n\
         env = { ANTHROPIC_API_KEY = \"x\" }\n",
    );

    let set = profiles::derive(dir.path());

    assert_eq!(
        why(&set, "x"),
        "profile 'x' sets ANTHROPIC_API_KEY, which is named like a credential — purlis \
         holds no credential in a profile, because anything set on the harness reaches the \
         model's own shell. Log in inside that harness instead: set CLAUDE_CONFIG_DIR and \
         run /login inside Claude Code."
    );
}

#[test]
fn a_profile_may_not_set_one_of_charters_own_variables() {
    purlis_core::unsteered!();
    // Ruling 14: a profile's `CHARTER_HARNESS` would tell every hook the wrong harness.
    let dir = plane(
        "",
        "[harness.x]\nkind = \"claude\"\ncommand = [\"claude\"]\n\
         env = { CHARTER_HARNESS = \"claude-code\" }\n",
    );

    let set = profiles::derive(dir.path());

    assert_eq!(
        why(&set, "x"),
        "profile 'x' sets CHARTER_HARNESS, one of purlis's own variables — purlis sets \
         those itself, and a profile's value would tell every hook the wrong harness or \
         plane. Remove it."
    );
}

#[test]
fn a_profile_may_set_neither_name_of_the_products_own_variables() {
    purlis_core::unsteered!();
    // The rename's window (V93k): `PURLIS_<X>` is the variable, `CHARTER_<X>` its old name,
    // and a profile may set neither — in any case, since Windows has none.
    for name in ["PURLIS_ROOT", "CHARTER_ROOT", "purlis_harness"] {
        let dir = plane(
            "",
            &format!(
                "[harness.x]\nkind = \"claude\"\ncommand = [\"claude\"]\n\
                 env = {{ {name} = \"/elsewhere\" }}\n"
            ),
        );

        let set = profiles::derive(dir.path());

        assert!(
            why(&set, "x").starts_with(&format!(
                "profile 'x' sets {name}, one of purlis's own variables"
            )),
            "{name}: {}",
            why(&set, "x")
        );
    }
}

#[test]
fn a_name_with_a_dot_is_refused_because_the_plane_format_fixes_the_alphabet() {
    purlis_core::unsteered!();
    let dir = plane(
        "",
        "[harness.\"has.dot\"]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
    );

    let set = profiles::derive(dir.path());

    assert_eq!(
        why(&set, "has.dot"),
        "profile 'has.dot' is not a name purlis accepts — letters, digits, '_' and '-', \
         starting with a letter or digit, and no dot — the plane format fixes that \
         alphabet. Rename the table."
    );
}

#[test]
fn a_section_other_than_harness_in_the_local_file_is_refused_by_name() {
    purlis_core::unsteered!();
    // An ignored file must not change plane policy with no trace in git — `[[forge]]` hosts
    // steer the one-credential guard.
    let dir = plane("", "[frame]\ndensity = \"wide\"\n");

    let set = profiles::derive(dir.path());

    assert_eq!(
        why(&set, "frame"),
        "[frame] in charter.local.toml is not read — that file carries [harness], \
         [extensions], [theme], [harness_plugins], [plane], [repos] and [chat_env] and \
         nothing else, because an ignored file must not change plane policy with no trace in git. \
         Put [frame] in charter.toml."
    );
}

#[test]
fn extensions_in_the_local_file_is_not_refused_by_the_profiles_loader() {
    purlis_core::unsteered!();
    // charter-app#253 (ADR 0048): `[extensions]` is this machine's choice among the extensions
    // it approved, read by `extension::project`, and is not the loader's to refuse.
    let dir = plane("", "[extensions.stats]\nenabled = false\n");

    let set = profiles::derive(dir.path());

    assert!(set.refused.is_empty(), "{:?}", set.refused);
}

#[test]
fn chat_env_in_the_local_file_is_not_refused_by_the_profiles_loader() {
    purlis_core::unsteered!();
    // `[chat_env]` is which more of this machine's environment a chat is started with, read by
    // `chatenv`, and not the loader's to refuse.
    let dir = plane("", "[chat_env]\npass = [\"JAVA_HOME\", \"GO*\"]\n");

    let set = profiles::derive(dir.path());

    assert!(set.refused.is_empty(), "{:?}", set.refused);
}

#[test]
fn a_chat_env_entry_that_is_not_a_name_is_refused_with_a_sentence() {
    purlis_core::unsteered!();
    let dir = plane("", "[chat_env]\npass = [\"*\"]\n");

    let set = profiles::derive(dir.path());

    assert_eq!(set.refused.len(), 1, "{:?}", set.refused);
    assert!(
        why(&set, "chat_env").contains("not a variable's name"),
        "{:?}",
        set.refused
    );
}

#[test]
fn harness_plugins_in_the_local_file_is_not_refused_by_the_profiles_loader() {
    purlis_core::unsteered!();
    // charter-app#274 (ADR 0050): `[harness_plugins]` is this machine's choice among the
    // plugins its harnesses have installed, read by `harness_plugin`, and not the loader's.
    let dir = plane("", "[harness_plugins.claude]\n\"figma@official\" = false\n");

    let set = profiles::derive(dir.path());

    assert!(set.refused.is_empty(), "{:?}", set.refused);
}

#[test]
fn a_theme_in_the_local_file_is_not_refused_by_the_profiles_loader() {
    purlis_core::unsteered!();
    // charter-app#273 (ADR 0048): `[theme]` is this machine's pick of the project's theme, read
    // by `extension::project::theme`, and is not the loader's to refuse.
    let dir = plane("", "[theme]\nuse = \"system\"\n");

    let set = profiles::derive(dir.path());

    assert!(set.refused.is_empty(), "{:?}", set.refused);
}

#[test]
fn a_local_file_that_does_not_parse_refuses_itself_and_leaves_the_built_ins_standing() {
    purlis_core::unsteered!();
    let dir = plane("", "this is not toml\n");

    let set = profiles::derive(dir.path());

    assert_eq!(names(&set), ["claude", "opencode", "codex"]);
    assert!(
        why(&set, "").starts_with("charter.local.toml could not be read ("),
        "{:?}",
        why(&set, "")
    );
}

#[test]
fn the_local_files_default_wins_over_the_committed_ones() {
    purlis_core::unsteered!();
    let dir = plane(
        "[harness]\ndefault = \"codex\"\n",
        "[harness]\ndefault = \"claude\"\n",
    );

    let set = profiles::derive(dir.path());

    assert_eq!(set.default.as_deref(), Some("claude"));
    assert_eq!(set.default_from.as_deref(), Some(profiles::LOCAL_FILE));
}

#[test]
fn a_default_naming_no_profile_this_machine_has_marks_no_row_rather_than_refusing_a_launch() {
    purlis_core::unsteered!();
    let dir = plane("[harness]\ndefault = \"claude-work\"\n", "");

    let set = profiles::derive(dir.path());

    assert_eq!(set.default, None);
    assert_eq!(set.default_refused.as_deref(), Some("claude-work"));
}

#[test]
fn a_profile_named_like_a_charter_command_is_refused_because_the_name_is_the_commands() {
    purlis_core::unsteered!();
    // `current`, not `derive`: this rule needs charter's own command words.
    let dir = plane(
        "",
        "[harness.save]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
    );

    let set = profiles::current(dir.path());

    assert_eq!(
        why(&set, "save"),
        "profile 'save' is named like the command `purlis save`, and that name belongs to \
         the command. Rename the table."
    );
    assert!(set.get("save").is_none());
}

#[test]
fn a_profile_that_runs_charter_itself_is_refused() {
    purlis_core::unsteered!();
    // Ruling 14. A profile names the harness a chat runs, and charter is not a harness.
    let dir = plane(
        "",
        "[harness.x]\nkind = \"claude\"\ncommand = [\"charter\", \"status\"]\n",
    );

    let set = profiles::current(dir.path());

    assert_eq!(
        why(&set, "x"),
        "profile 'x' runs purlis itself — a profile names the harness a chat runs, and \
         purlis is not a harness. Give it the harness's own command."
    );
}

#[test]
fn charter_is_recognised_however_it_is_spelt_including_through_python() {
    purlis_core::unsteered!();
    // `charter/hooks.py:840` — `charter`, `edm`, and `python -m charter`, case-folded,
    // because on a case-insensitive filesystem `CHARTER` runs the same binary.
    for command in [
        "[\"charter\"]",
        "[\"edm\", \"status\"]",
        "[\"/usr/bin/CHARTER\"]",
        "[\"python3\", \"-m\", \"charter\"]",
        // The name the command line ships as since RN-3, spelt every way the old one is.
        "[\"purlis\"]",
        "[\"/usr/bin/PURLIS\", \"status\"]",
        "[\"python3\", \"-m\", \"purlis\"]",
    ] {
        let dir = plane(
            "",
            &format!("[harness.x]\nkind = \"claude\"\ncommand = {command}\n"),
        );

        let set = profiles::current(dir.path());

        assert!(
            set.get("x").is_none(),
            "{command} was taken as a harness charter could launch"
        );
    }
}

#[test]
fn a_tilde_is_expanded_at_the_launch_and_never_in_the_file() {
    purlis_core::unsteered!();
    // The file is what an edit changes, and a home directory that moved would otherwise
    // make every profile read as CHANGED and ask again about a command nobody touched.
    let dir = plane(
        "",
        "[harness.x]\nkind = \"claude\"\ncommand = [\"~/bin/claude\", \"~/notes\"]\n\
         env = { CLAUDE_CONFIG_DIR = \"~/.claude-work\" }\n",
    );
    let set = profiles::derive(dir.path());
    let p = set.get("x").unwrap();

    assert_eq!(p.command, ["~/bin/claude", "~/notes"]);
    assert_eq!(
        profiles::expanded_command(p, Path::new("/home/a")),
        ["/home/a/bin/claude", "~/notes"],
        "only the program is expanded — an argument is the harness's to interpret"
    );
    assert_eq!(
        profiles::expanded_env(p, Path::new("/home/a")).get("CLAUDE_CONFIG_DIR"),
        Some(&"/home/a/.claude-work".to_owned())
    );
}

#[test]
fn a_profile_is_shown_as_its_environment_then_a_command_a_person_could_paste() {
    purlis_core::unsteered!();
    // `charter/profiles.py:486`, taken from the oracle — except the environment value, which
    // is quoted as the command's words are (#1014): unquoted, `A=b c` reads as `A=b` and a
    // program `c`, so two different profiles could show one line.
    let dir = plane(
        "",
        "[harness.x]\nkind = \"claude\"\n\
         command = [\"~/.local/bin/claude\", \"--model\", \"opus 4\"]\n\
         env = { A = \"b c\" }\n",
    );
    let set = profiles::derive(dir.path());

    assert_eq!(
        profiles::display(set.get("x").unwrap()),
        "A='b c' ~/.local/bin/claude --model 'opus 4'"
    );
}

#[test]
fn a_leading_tilde_stays_bare_so_the_line_can_be_pasted_into_a_shell() {
    purlis_core::unsteered!();
    // #1004's proof run: `shlex.quote` puts a leading `~` INSIDE quotes, where a shell does
    // not expand it, so `'~/.local/bin/codex'` pasted into a terminal names a directory
    // called `~`. The `~/` stays bare and the rest is quoted.
    let dir = plane(
        "",
        "[harness.x]\nkind = \"claude\"\ncommand = [\"~/a b/claude\"]\n",
    );
    let set = profiles::derive(dir.path());

    assert_eq!(profiles::display(set.get("x").unwrap()), "~/'a b/claude'");
}

#[test]
fn another_users_home_stays_quoted_whole_because_charter_does_not_guess_which_home() {
    purlis_core::unsteered!();
    let dir = plane(
        "",
        "[harness.x]\nkind = \"claude\"\ncommand = [\"~other/bin/claude\"]\n",
    );
    let set = profiles::derive(dir.path());

    assert_eq!(
        profiles::display(set.get("x").unwrap()),
        "'~other/bin/claude'"
    );
}

#[test]
fn a_control_byte_in_a_command_is_shown_escaped_and_never_redraws_the_row() {
    purlis_core::unsteered!();
    // Ruling 35: `charter.local.toml` is a file a chat can write, and the selector draws
    // this line. The quoting happens first and the escaping second, as the oracle does it.
    let dir = plane(
        "",
        "[harness.x]\nkind = \"claude\"\ncommand = [\"cl\\naude\"]\n",
    );
    let set = profiles::derive(dir.path());

    assert_eq!(profiles::display(set.get("x").unwrap()), "'cl\\u000aaude'");
}

// ---------------------------------------------------------------------------
// Added after an adversarial review found each of these guards untested: every
// mutation below survived the suite as it first stood. Expectations taken from
// the oracle, run over the same declarations.
// ---------------------------------------------------------------------------

#[test]
fn every_word_that_names_a_credential_is_refused_and_not_only_the_one_that_was_tested() {
    purlis_core::unsteered!();
    // The guard is four words and only `KEY` was ever exercised, through
    // `ANTHROPIC_API_KEY` — so dropping `PASSWORD` from the list reddened nothing.
    for (var, kind) in [
        ("ANTHROPIC_API_KEY", "KEY"),
        ("GH_TOKEN", "TOKEN"),
        ("MY_SECRET", "SECRET"),
        ("DB_PASSWORD", "PASSWORD"),
    ] {
        let dir = plane(
            "",
            &format!(
                "[harness.x]\nkind = \"claude\"\ncommand = [\"claude\"]\n\
                 env = {{ {var} = \"x\" }}\n"
            ),
        );

        let set = profiles::derive(dir.path());

        assert!(
            why(&set, "x").starts_with(&format!(
                "profile 'x' sets {var}, which is named like a credential"
            )),
            "{kind} was not refused through {var}"
        );
    }
}

#[test]
fn a_credential_name_is_caught_whichever_case_it_is_written_in() {
    purlis_core::unsteered!();
    // The match is case-folded, and that folding is the guard's whole point: `my_api_token`
    // reaches the model's shell exactly as `MY_API_TOKEN` would.
    let dir = plane(
        "",
        "[harness.x]\nkind = \"claude\"\ncommand = [\"claude\"]\n\
         env = { my_api_token = \"x\" }\n",
    );

    let set = profiles::derive(dir.path());

    assert!(
        why(&set, "x")
            .starts_with("profile 'x' sets my_api_token, which is named like a credential"),
        "{:?}",
        why(&set, "x")
    );
}

#[test]
fn a_command_holding_an_empty_word_has_no_usable_command() {
    purlis_core::unsteered!();
    let dir = plane(
        "",
        "[harness.x]\nkind = \"claude\"\ncommand = [\"claude\", \"\"]\n",
    );

    assert!(profiles::derive(dir.path()).get("x").is_none());
}

#[test]
fn an_empty_command_list_has_no_usable_command() {
    purlis_core::unsteered!();
    // And this rule is the only thing standing between `charter harness list` and a profile
    // with no program to show. Both halves are pinned: the refusal, and that showing such a
    // profile answers rather than ending the process.
    let dir = plane("", "[harness.x]\nkind = \"claude\"\ncommand = []\n");
    let set = profiles::derive(dir.path());

    assert!(set.get("x").is_none());
    assert_eq!(
        profiles::display(&profiles::Profile {
            name: "x".into(),
            kind: "claude".into(),
            harness: "claude-code".into(),
            command: Vec::new(),
            env: vec![("A".into(), "b".into())],
            source: profiles::Source::Local,
        }),
        "A=b"
    );
}

#[test]
fn a_kind_that_is_not_text_is_quoted_back_the_way_the_oracle_quotes_it() {
    purlis_core::unsteered!();
    // A refusal that quotes a value back differently is the second answer this port exists
    // to prevent, however wrong the value being quoted is.
    for (declared, shown) in [
        ("true", "True"),
        ("[\"claude\"]", "['claude']"),
        ("{ a = 1 }", "{'a': 1}"),
        ("7", "7"),
    ] {
        let dir = plane(
            "",
            &format!("[harness.x]\nkind = {declared}\ncommand = [\"claude\"]\n"),
        );

        assert_eq!(
            why(&profiles::derive(dir.path()), "x"),
            format!(
                "profile 'x' has kind {shown}, which is not a harness purlis can launch — \
                 one of: claude, opencode, codex. Set kind to one of them."
            ),
            "kind = {declared}"
        );
    }
}

#[test]
fn a_control_byte_in_an_environment_value_is_shown_escaped_too() {
    purlis_core::unsteered!();
    // The command piece was contained and checked; the `NAME=value` pieces were contained
    // and not checked, so the containment could have been dropped from them unnoticed.
    let dir = plane(
        "",
        "[harness.x]\nkind = \"claude\"\ncommand = [\"claude\"]\n\
         env = { A = \"\\u001b[2K\\rEVIL\" }\n",
    );
    let set = profiles::derive(dir.path());

    assert_eq!(
        profiles::display(set.get("x").unwrap()),
        "A='\\u001b[2K\\u000dEVIL' claude"
    );
}

#[test]
fn charter_is_recognised_through_python_whatever_case_the_module_is_written_in() {
    purlis_core::unsteered!();
    // The program's casing was pinned and the MODULE's was not, so folding it could have
    // been dropped and `python3 -m CHARTER` would have become a profile charter launches.
    let dir = plane(
        "",
        "[harness.x]\nkind = \"claude\"\ncommand = [\"python3\", \"-m\", \"CHARTER\"]\n",
    );

    assert!(profiles::current(dir.path()).get("x").is_none());
}

#[test]
fn a_default_that_stands_leaves_no_refused_default_beside_it() {
    purlis_core::unsteered!();
    // The two are mutually exclusive. A `default` refused in the committed file used to
    // survive beside a good one from the local file, and the first surface to print it
    // would have told an operator their working default named nothing.
    let dir = plane(
        "[harness]\ndefault = 7\n",
        "[harness]\ndefault = \"claude\"\n",
    );

    let set = profiles::derive(dir.path());

    assert_eq!(set.default.as_deref(), Some("claude"));
    assert_eq!(set.default_refused, None);
}

// ---------------------------------------------------------------------------
// Four guards the nightly mutation run found with no test behind them (charter-app
// run 35430920851): each mutation below survived the whole suite. Every sentence is
// the one `charter/profiles.py` writes for the same declaration.
// ---------------------------------------------------------------------------

#[test]
fn a_default_the_committed_file_names_stands_when_this_machine_has_that_profile() {
    purlis_core::unsteered!();
    // Every committed default tested so far was either overruled by the local file or named
    // a profile nobody has — and both of those end with no default at all, so deleting the
    // `!` that keeps a TEXT default (profiles.rs, `if !value.is_str()`) passed everything.
    let dir = plane("[harness]\ndefault = \"codex\"\n", "");

    let set = profiles::derive(dir.path());

    assert_eq!(set.default.as_deref(), Some("codex"));
    assert_eq!(set.default_from.as_deref(), Some(profiles::COMMITTED_FILE));
    assert_eq!(set.default_refused, None);
}

#[test]
fn a_default_in_the_committed_file_that_is_not_text_is_refused_by_value() {
    purlis_core::unsteered!();
    // The other half of the same guard. A `default = 7` names nothing, and a surface must be
    // able to say what it named — `7`, quoted back the way the oracle quotes it.
    let dir = plane("[harness]\ndefault = 7\n", "");

    let set = profiles::derive(dir.path());

    assert_eq!(set.default, None);
    assert_eq!(set.default_refused.as_deref(), Some("7"));
}

#[test]
fn a_local_file_that_is_there_and_cannot_be_read_is_refused_rather_than_taken_as_absent() {
    purlis_core::unsteered!();
    // Only an ABSENT file declares nothing. Anything else that stops the read is a refusal,
    // because a file that is there and says nothing looks exactly like one nobody wrote —
    // and the operator's `claude-work` profile would silently not exist. Widening the
    // `NotFound` guard to every error passed the suite; a directory in the file's place is
    // an error on every platform that is not `NotFound`.
    let dir = plane("", "");
    fs::create_dir(dir.path().join(profiles::LOCAL_FILE)).unwrap();

    let set = profiles::derive(dir.path());

    let refused = set
        .refused
        .iter()
        .find(|r| r.name.is_empty())
        .unwrap_or_else(|| panic!("an unreadable file was taken as absent: {:?}", set.refused));
    assert_eq!(refused.source, profiles::LOCAL_FILE);
    assert!(
        refused
            .reason
            .starts_with("charter.local.toml could not be read ("),
        "{:?}",
        refused.reason
    );
    assert!(
        refused.reason.ends_with(
            "), so no declared profile was loaded — the built-in profiles still are. Fix the \
             file and run purlis harness list."
        ),
        "{:?}",
        refused.reason
    );
    assert_eq!(names(&set), ["claude", "opencode", "codex"]);
}

#[test]
fn a_table_named_default_is_refused_as_a_profile_and_is_not_read_as_the_default() {
    purlis_core::unsteered!();
    // `default` is a key only while it is not a table. `[harness.default]` is a PROFILE
    // declaration under the one name a profile cannot take, and it is refused as one —
    // rather than swallowed as a default that names a table. Turning the guard's `&&` into
    // `||` did exactly that swallowing and passed the suite.
    let dir = plane(
        "",
        "[harness.default]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
    );

    let set = profiles::derive(dir.path());

    assert_eq!(
        why(&set, "default"),
        "a profile cannot be named 'default' — `default` is the one key under [harness] that \
         is not a profile. Rename the table."
    );
    assert_eq!(set.default, None);
    assert_eq!(set.default_from, None);
    assert_eq!(set.default_refused, None);
}

#[test]
fn a_bare_value_under_harness_is_refused_as_not_a_table_and_never_becomes_the_default() {
    purlis_core::unsteered!();
    // The same `&&`, from the other side: with `||`, ANY non-table key under [harness] was
    // read as `default`, so `work = "claude"` — a half-written profile — silently made
    // `claude` the row every new chat starts on, and the operator was told nothing.
    let dir = plane("", "[harness]\nwork = \"claude\"\n");

    let set = profiles::derive(dir.path());

    assert_eq!(
        why(&set, "work"),
        "[harness] work in charter.local.toml is not a table — a profile is [harness.work] \
         with kind, command and optionally env."
    );
    assert_eq!(set.default, None);
    assert_eq!(set.default_from, None);
}

#[test]
fn a_profiles_own_env_table_is_not_mistaken_for_a_nested_profile() {
    purlis_core::unsteered!();
    // `env` is the one key a profile has whose value IS a table, and the nested-table check
    // must leave it alone. Every test declaring an `env` looked the profile up by name and
    // never asked what else was refused — so an `||` that flagged `env` as a dotted name
    // (`[harness.work] holds a table env, …`) passed, and every profile with an environment
    // arrived with a refusal about itself beside it.
    let dir = plane(
        "",
        "[harness.work]\nkind = \"claude\"\ncommand = [\"claude\"]\n\
         env = { CLAUDE_CONFIG_DIR = \"~/.claude-work\" }\n",
    );

    let set = profiles::derive(dir.path());

    assert_eq!(set.refused, []);
    assert_eq!(
        set.get("work").unwrap().env,
        [("CLAUDE_CONFIG_DIR".to_owned(), "~/.claude-work".to_owned())]
    );
}

#[test]
fn an_env_holding_a_value_that_is_not_text_refuses_the_profile() {
    purlis_core::unsteered!();
    // Same run, the same file. `env = { A = 1 }` was let through as `A=""` once the guard
    // on every value being text was widened to `true` — the operator's value replaced by
    // nothing, and the chat started anyway.
    let dir = plane(
        "",
        "[harness.x]\nkind = \"claude\"\ncommand = [\"claude\"]\nenv = { A = 1 }\n",
    );

    let set = profiles::derive(dir.path());

    assert_eq!(
        why(&set, "x"),
        "profile 'x' has an env that is not a table of text values — write env = { NAME = \
         \"value\" }."
    );
    assert!(set.get("x").is_none());
}

#[test]
fn a_quoted_back_value_holding_an_apostrophe_is_quoted_the_way_python_quotes_it() {
    purlis_core::unsteered!();
    // Python's `repr` switches to double quotes for a string holding a `'` and no `"` —
    // `["it's"]`, not `['it\'s']`. Only strings with neither were ever quoted back, so the
    // switch could be deleted unnoticed and the two implementations would give two answers
    // about the same file.
    let dir = plane(
        "",
        "[harness.x]\nkind = [\"it's\"]\ncommand = [\"claude\"]\n",
    );

    assert_eq!(
        why(&profiles::derive(dir.path()), "x"),
        "profile 'x' has kind [\"it's\"], which is not a harness purlis can launch — one of: \
         claude, opencode, codex. Set kind to one of them."
    );
}

#[test]
fn the_launch_read_is_the_one_that_has_already_asked_git() {
    purlis_core::unsteered!();
    // `current` is the unchecked read; pairing it with the git check by hand is a pairing
    // one caller will forget, and what that lets through is a command out of a file every
    // clone of this plane carries.
    let dir = plane(
        "",
        "[harness.work]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
    );
    purlis_core::forklock::output(
        support::unsigned()
            .args(["init", "-q"])
            .current_dir(dir.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null"),
    )
    .expect("git runs");

    let (set, check) = profiles::for_launch(dir.path());

    assert!(
        set.get("work").is_none(),
        "a git-carried profile reached a launch"
    );
    assert_eq!(check.fix, "purlis reinit");
    assert!(
        profiles::current(dir.path()).get("work").is_some(),
        "the unchecked read is still the unchecked read"
    );
}

/// A local profile whose every piece — an environment value and the command — runs past
/// [`purlis_core::shown::DISPLAY_LIMIT`], with a last word a clip would cut off.
fn a_long_profile() -> profiles::Profile {
    let filler = "x".repeat(purlis_core::shown::DISPLAY_LIMIT);
    profiles::Profile {
        name: "x".into(),
        kind: "claude".into(),
        harness: "claude-code".into(),
        command: vec![
            "claude".into(),
            "--note".into(),
            filler.clone(),
            "--and-then".into(),
            "the-last-word".into(),
        ],
        env: vec![("A".into(), format!("{filler}-env-tail"))],
        source: Source::Local,
    }
}

#[test]
fn approving_a_profile_shows_every_word_of_a_command_longer_than_the_display_limit() {
    purlis_core::unsteered!();
    // #1014: the approval line is what the operator says yes to, and the whole command is
    // what then runs, so the line is never clipped at the display limit — not the command
    // and not an environment value.
    let dir = tempfile::tempdir().unwrap();
    let p = a_long_profile();
    let filler = "x".repeat(purlis_core::shown::DISPLAY_LIMIT);

    let shown = purlis_core::profiletrust::shown(dir.path(), &p);

    assert_eq!(
        shown,
        format!(
            "A={filler}-env-tail claude --note {filler} --and-then the-last-word (kind claude)"
        )
    );
    assert_eq!(
        shown,
        format!("{} (kind claude)", profiles::display_whole(&p))
    );
}

#[test]
fn an_approval_of_the_clipped_line_is_refused_and_the_whole_line_is_approved() {
    purlis_core::unsteered!();
    // The click is checked against the whole line, so a yes to a line that stopped short of
    // the command records nothing.
    let dir = tempfile::tempdir().unwrap();
    let p = a_long_profile();

    let clipped = profiles::display(&p);
    assert!(
        clipped.contains("..."),
        "the listing still clips: {clipped}"
    );
    let refused = purlis_core::profiletrust::approve(dir.path(), &p, &clipped)
        .expect_err("a yes to a clipped line was recorded");
    assert!(refused.contains("the-last-word"), "{refused}");
    assert!(
        purlis_core::profiletrust::approval_needed(dir.path(), &p).is_some(),
        "the refused yes recorded an approval"
    );

    purlis_core::profiletrust::approve(
        dir.path(),
        &p,
        &purlis_core::profiletrust::shown(dir.path(), &p),
    )
    .expect("the whole line is approved");
    assert_eq!(
        purlis_core::profiletrust::approval_needed(dir.path(), &p),
        None
    );
}

/// A local profile `x` of kind `claude` with `env` and `command`, as the loader builds one.
fn local(env: &[(&str, &str)], command: &[&str]) -> profiles::Profile {
    profiles::Profile {
        name: "x".into(),
        kind: "claude".into(),
        harness: "claude-code".into(),
        command: command.iter().map(|w| (*w).to_owned()).collect(),
        env: env
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect(),
        source: Source::Local,
    }
}

#[test]
fn where_an_environment_value_ends_and_the_command_begins_reads_one_way() {
    purlis_core::unsteered!();
    // #1014: two profiles that run different programs never show the same approval line.
    let dir = tempfile::tempdir().unwrap();
    let shown_one = local(&[("A", "1")], &["claude", "x"]);
    let on_disk = local(&[("A", "1 claude")], &["x"]);

    let line = purlis_core::profiletrust::shown(dir.path(), &shown_one);

    assert_eq!(line, "A=1 claude x (kind claude)");
    assert_eq!(
        purlis_core::profiletrust::shown(dir.path(), &on_disk),
        "A='1 claude' x (kind claude)"
    );
    // A yes to the first line does not approve the second profile.
    let refused = purlis_core::profiletrust::approve(dir.path(), &on_disk, &line)
        .expect_err("a yes to one profile's line approved another");
    assert!(
        refused.contains("changed while you were reading it"),
        "{refused}"
    );
    assert!(
        purlis_core::profiletrust::approval_needed(dir.path(), &on_disk).is_some(),
        "the refused yes recorded an approval"
    );
}

#[test]
fn an_environment_value_keeps_a_leading_home_bare_because_purlis_expands_it() {
    purlis_core::unsteered!();
    assert_eq!(
        profiles::display_whole(&local(&[("D", "~/a b")], &["claude"])),
        "D=~/'a b' claude"
    );
}

#[test]
fn an_environment_name_a_shell_could_not_set_is_refused() {
    purlis_core::unsteered!();
    // A quoted TOML key holds anything; a name with a space or `=` would move where the
    // approval line's `NAME=value` piece seems to end.
    for name in ["A B", "A=1", "1A", "", "A-B", "Ä"] {
        let dir = plane(
            "",
            &format!(
                "[harness.x]\nkind = \"claude\"\ncommand = [\"claude\"]\n\
                 env = {{ {name:?} = \"v\" }}\n"
            ),
        );

        let set = profiles::derive(dir.path());

        assert!(set.get("x").is_none(), "{name:?} was accepted");
        assert!(
            why(&set, "x").contains("which is not an environment variable name"),
            "{name:?}: {}",
            why(&set, "x")
        );
    }
    for name in ["A", "_A", "a_1"] {
        let dir = plane(
            "",
            &format!(
                "[harness.x]\nkind = \"claude\"\ncommand = [\"claude\"]\n\
                 env = {{ {name} = \"v\" }}\n"
            ),
        );

        assert!(
            profiles::derive(dir.path()).get("x").is_some(),
            "{name} was refused"
        );
    }
}

#[test]
fn a_first_word_shaped_like_an_assignment_never_reads_as_an_environment_variable() {
    purlis_core::unsteered!();
    // #1014: purlis runs a first word `A=/x` as the program, and a shell reads it bare as a
    // variable, so it is drawn quoted — and the profiles below never share a line.
    let dir = tempfile::tempdir().unwrap();
    let groups = [
        vec![
            local(&[("A", "/x")], &["claude"]),
            local(&[], &["A=/x", "claude"]),
        ],
        vec![
            local(&[("A", "x"), ("B", "y")], &["c"]),
            local(&[("A", "x")], &["B=y", "c"]),
            local(&[], &["A=x", "B=y", "c"]),
        ],
    ];
    for group in &groups {
        let lines: Vec<String> = group
            .iter()
            .map(|p| purlis_core::profiletrust::shown(dir.path(), p))
            .collect();
        for (i, one) in lines.iter().enumerate() {
            for (j, other) in lines.iter().enumerate() {
                if i == j {
                    continue;
                }
                assert_ne!(one, other, "two profiles share a line");
                // A yes to one profile's line does not approve another.
                purlis_core::profiletrust::approve(dir.path(), &group[j], one)
                    .expect_err("a yes to one profile's line approved another");
            }
        }
        assert!(
            group
                .iter()
                .all(|p| { purlis_core::profiletrust::approval_needed(dir.path(), p).is_some() })
        );
    }
    assert_eq!(
        profiles::display_whole(&local(&[], &["A=/x", "claude"])),
        "'A=/x' claude"
    );
    assert_eq!(
        profiles::display_whole(&local(&[("A", "x")], &["B=y", "c"])),
        "A=x 'B=y' c"
    );
    // A word that only holds `=` past a name a shell could not set stays as it was.
    assert_eq!(
        profiles::display_whole(&local(&[], &["/opt/a=b/claude", "--x=y"])),
        "/opt/a=b/claude --x=y"
    );
}

#[test]
fn two_profiles_that_differ_only_in_kind_never_share_an_approval_line() {
    purlis_core::unsteered!();
    // #1014: the approval records the kind, and the kind chooses the harness — the words
    // purlis adds, the sandbox, the guard — so the line the operator approves says it.
    let dir = tempfile::tempdir().unwrap();
    let claude = local(&[("A", "1")], &["run"]);
    let codex = profiles::Profile {
        kind: "codex".into(),
        harness: "codex".into(),
        ..claude.clone()
    };

    let claude_line = purlis_core::profiletrust::shown(dir.path(), &claude);
    let codex_line = purlis_core::profiletrust::shown(dir.path(), &codex);

    assert_eq!(claude_line, "A=1 run (kind claude)");
    assert_eq!(codex_line, "A=1 run (kind codex)");
    purlis_core::profiletrust::approve(dir.path(), &codex, &claude_line)
        .expect_err("a yes to the claude line approved the codex profile");
    assert!(purlis_core::profiletrust::approval_needed(dir.path(), &codex).is_some());
    purlis_core::profiletrust::approve(dir.path(), &codex, &codex_line)
        .expect("its own line is approved");
    assert_eq!(
        purlis_core::profiletrust::approval_needed(dir.path(), &codex),
        None
    );
}
