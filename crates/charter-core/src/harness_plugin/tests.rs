//! The harness-plugin model (charter-app#274, ADR 0050): the precedence, each adapter's listing
//! from a fixture directory, and what a file may not say. Every expected answer is written out.

use super::*;
use crate::extension::project::Source;

fn plugin(harness: &'static str, id: &str) -> Plugin {
    Plugin {
        id: id.to_owned(),
        harness,
        name: id.split('@').next().unwrap_or(id).to_owned(),
        source: "fixture".to_owned(),
    }
}

fn claude(installed: &[&str], shared: &str, local: &str) -> Vec<Effective> {
    let installed: Vec<Plugin> = installed.iter().map(|id| plugin("claude", id)).collect();
    resolve(
        &CLAUDE_CODE,
        &installed,
        &Choices::from_text(Some(shared), Some(local)),
    )
}

fn row<'a>(all: &'a [Effective], id: &str) -> &'a Effective {
    all.iter()
        .find(|it| it.id == id)
        .unwrap_or_else(|| panic!("{id} is not listed: {all:?}"))
}

// -------------------------------------------------------------------------------------
// The precedence: Local over Shared, per plugin; neither is "not set"
// -------------------------------------------------------------------------------------

#[test]
fn an_installed_plugin_no_file_names_is_not_set_and_left_to_the_harness() {
    let all = claude(&["figma@official"], "", "");
    let it = row(&all, "figma@official");
    assert_eq!((it.wanted, it.source), (None, Source::Default));
    assert!(it.installed);
}

#[test]
fn shared_turns_a_plugin_on_or_off() {
    let all = claude(
        &["figma@official", "serena@official"],
        "[harness_plugins.claude]\n\"figma@official\" = false\n\"serena@official\" = true\n",
        "",
    );
    assert_eq!(
        (
            row(&all, "figma@official").wanted,
            row(&all, "figma@official").source
        ),
        (Some(false), Source::Shared)
    );
    assert_eq!(
        (
            row(&all, "serena@official").wanted,
            row(&all, "serena@official").source
        ),
        (Some(true), Source::Shared)
    );
}

#[test]
fn local_overrides_shared_in_both_directions() {
    let all = claude(
        &["figma@official", "serena@official"],
        "[harness_plugins.claude]\n\"figma@official\" = false\n\"serena@official\" = true\n",
        "[harness_plugins.claude]\n\"figma@official\" = true\n\"serena@official\" = false\n",
    );
    assert_eq!(
        (
            row(&all, "figma@official").wanted,
            row(&all, "figma@official").source
        ),
        (Some(true), Source::Local)
    );
    assert_eq!(
        (
            row(&all, "serena@official").wanted,
            row(&all, "serena@official").source
        ),
        (Some(false), Source::Local)
    );
}

#[test]
fn local_naming_one_plugin_leaves_shared_in_charge_of_the_others() {
    // Per plugin, as #272 is per key: Local saying something about one plugin is not Local
    // taking over the harness's whole table.
    let all = claude(
        &["figma@official", "serena@official"],
        "[harness_plugins.claude]\n\"figma@official\" = false\n",
        "[harness_plugins.claude]\n\"serena@official\" = true\n",
    );
    assert_eq!(
        (
            row(&all, "figma@official").wanted,
            row(&all, "figma@official").source
        ),
        (Some(false), Source::Shared)
    );
}

#[test]
fn a_choice_for_another_harness_does_not_reach_this_one() {
    let all = claude(
        &["figma@official"],
        "[harness_plugins.codex]\n\"figma@official\" = false\n",
        "",
    );
    assert_eq!(row(&all, "figma@official").wanted, None);
}

#[test]
fn a_plugin_a_file_names_that_is_not_installed_is_listed_and_handed_to_nothing() {
    let all = claude(&[], "[harness_plugins.claude]\n\"acme@corp\" = true\n", "");
    let it = row(&all, "acme@corp");
    assert!(!it.installed);
    assert_eq!((it.wanted, it.source), (Some(true), Source::Shared));
    assert_eq!(
        chosen(&CLAUDE_CODE, &all),
        BTreeMap::from(pins(&CLAUDE_CODE))
    );
}

// -------------------------------------------------------------------------------------
// What no file moves
// -------------------------------------------------------------------------------------

fn pins(adapter: &dyn Adapter) -> [(String, bool); 2] {
    let pinned = adapter.pinned();
    assert_eq!(pinned.len(), 2, "{pinned:?}");
    [
        (pinned[0].id.to_owned(), pinned[0].on),
        (pinned[1].id.to_owned(), pinned[1].on),
    ]
}

#[test]
fn charters_own_plugin_is_always_on_and_the_old_one_always_off() {
    let all = claude(&["charter@charter"], "", "");
    assert_eq!(row(&all, "charter-app@inline").wanted, Some(true));
    assert!(row(&all, "charter-app@inline").pinned.is_some());
    assert_eq!(row(&all, "charter@charter").wanted, Some(false));
    assert!(row(&all, "charter@charter").pinned.is_some());
}

#[test]
fn no_file_can_turn_charters_own_plugin_off_or_the_old_one_on() {
    let all = claude(
        &["charter@charter"],
        "[harness_plugins.claude]\n\"charter-app@inline\" = false\n",
        "[harness_plugins.claude]\n\"charter@charter\" = true\n",
    );
    let own = row(&all, "charter-app@inline");
    assert_eq!((own.wanted, own.source), (Some(true), Source::Default));
    assert_eq!(
        own.ignored,
        [Ignored {
            source: Source::Shared,
            why: "charter.toml sets harness_plugins.claude.\"charter-app@inline\" to false, and \
                  charter-app@inline is always on: it is charter's own plugin, and it carries \
                  charter's hooks and the Bash guard"
                .to_owned(),
        }]
    );
    let old = row(&all, "charter@charter");
    assert_eq!((old.wanted, old.source), (Some(false), Source::Default));
    assert_eq!(old.ignored.len(), 1, "{:?}", old.ignored);
    assert_eq!(old.ignored[0].source, Source::Local);

    let chosen = chosen(&CLAUDE_CODE, &all);
    assert_eq!(chosen.get("charter-app@inline"), Some(&true));
    assert_eq!(chosen.get("charter@charter"), Some(&false));
}

#[test]
fn a_chat_gets_exactly_the_installed_plugins_a_file_decided_and_the_pins() {
    let all = claude(
        &["figma@official", "serena@official", "humanizer@h"],
        "[harness_plugins.claude]\n\"figma@official\" = false\n\"serena@official\" = true\n",
        "[harness_plugins.claude]\n\"serena@official\" = false\n",
    );
    assert_eq!(
        chosen(&CLAUDE_CODE, &all),
        BTreeMap::from([
            ("charter-app@inline".to_owned(), true),
            ("charter@charter".to_owned(), false),
            ("figma@official".to_owned(), false),
            ("serena@official".to_owned(), false),
        ])
    );
}

// -------------------------------------------------------------------------------------
// A harness whose adapter cannot apply says so
// -------------------------------------------------------------------------------------

#[test]
fn every_harness_charter_knows_has_an_adapter_and_says_whether_it_applies() {
    let words: Vec<&str> = ADAPTERS.iter().map(|it| it.harness()).collect();
    assert_eq!(words, ["claude", "opencode", "codex"]);
    assert!(matches!(CLAUDE_CODE.support(), Support::PerChat));
    for adapter in [&CODEX as &dyn Adapter, &OPENCODE] {
        let Support::NotYet(why) = adapter.support() else {
            panic!("{} claims to apply", adapter.title());
        };
        assert!(!why.is_empty());
    }
}

#[test]
fn a_harness_that_cannot_apply_says_plugins_are_not_supported_yet() {
    assert_eq!(
        not_supported(&CODEX).expect("Codex cannot apply"),
        format!(
            "plugins for Codex are not supported yet — {}",
            match CODEX.support() {
                Support::NotYet(why) => why,
                Support::PerChat => unreachable!(),
            }
        )
    );
    assert!(
        not_supported(&OPENCODE)
            .expect("opencode cannot apply")
            .starts_with("plugins for opencode are not supported yet — ")
    );
    assert_eq!(not_supported(&CLAUDE_CODE), None);
}

#[test]
fn a_chat_on_a_harness_that_cannot_apply_is_handed_nothing_and_its_choices_are_said_ignored() {
    let installed = [plugin("codex", "charter@charter")];
    let all = resolve(
        &CODEX,
        &installed,
        &Choices::from_text(
            Some("[harness_plugins.codex]\n\"charter@charter\" = false\n"),
            None,
        ),
    );
    assert_eq!(chosen(&CODEX, &all), BTreeMap::new());
    let it = row(&all, "charter@charter");
    assert_eq!(it.ignored.len(), 1, "{:?}", it.ignored);
    assert!(
        it.ignored[0]
            .why
            .contains("plugins for Codex are not supported yet"),
        "{}",
        it.ignored[0].why
    );
}

// -------------------------------------------------------------------------------------
// The adapters' listings, from fixtures
// -------------------------------------------------------------------------------------

fn env_of(pairs: &[(&str, &std::path::Path)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, path)| ((*name).to_owned(), path.display().to_string()))
        .collect()
}

#[test]
fn claude_code_lists_every_plugin_its_install_record_holds_once() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join("plugins")).unwrap();
    std::fs::write(
        home.path().join("plugins/installed_plugins.json"),
        r#"{"version": 2, "plugins": {
            "figma@claude-plugins-official": [{"scope": "user", "installPath": "/x", "version": "2.2.1"}],
            "superpowers@claude-plugins-official": [
                {"scope": "project", "projectPath": "/a", "installPath": "/y", "version": "5.0.0"},
                {"scope": "project", "projectPath": "/c", "installPath": "/y", "version": "5.0.0"},
                {"scope": "local", "projectPath": "/b", "installPath": "/y", "version": "5.0.0"}
            ]
        }}"#,
    )
    .unwrap();
    let env = env_of(&[("CLAUDE_CONFIG_DIR", home.path())]);

    let got = CLAUDE_CODE.installed(&Env::of(&env)).expect("it reads");

    assert_eq!(
        got,
        [
            Plugin {
                id: "figma@claude-plugins-official".to_owned(),
                harness: "claude",
                name: "figma".to_owned(),
                source: "claude-plugins-official, user".to_owned(),
            },
            Plugin {
                id: "superpowers@claude-plugins-official".to_owned(),
                harness: "claude",
                name: "superpowers".to_owned(),
                source: "claude-plugins-official, project, local".to_owned(),
            },
        ]
    );
}

#[test]
fn claude_code_with_nothing_installed_lists_nothing() {
    let home = tempfile::tempdir().unwrap();
    let env = env_of(&[("CLAUDE_CONFIG_DIR", home.path())]);
    assert_eq!(CLAUDE_CODE.installed(&Env::of(&env)), Ok(Vec::new()));
}

#[test]
fn claude_code_says_so_when_its_install_record_is_not_json() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join("plugins")).unwrap();
    std::fs::write(home.path().join("plugins/installed_plugins.json"), "{").unwrap();
    let env = env_of(&[("CLAUDE_CONFIG_DIR", home.path())]);

    let why = CLAUDE_CODE
        .installed(&Env::of(&env))
        .expect_err("it refuses");
    assert!(why.contains("installed_plugins.json"), "{why}");
}

#[test]
fn claude_code_reads_under_home_when_the_chat_names_no_config_dir() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".claude/plugins")).unwrap();
    std::fs::write(
        home.path().join(".claude/plugins/installed_plugins.json"),
        r#"{"version": 2, "plugins": {"a@m": [{"scope": "user"}]}}"#,
    )
    .unwrap();
    let env = Env {
        chat: &[],
        home: Some(home.path().to_path_buf()),
        process: false,
    };

    let got = CLAUDE_CODE.installed(&env).expect("it reads");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].id, "a@m");
}

#[test]
fn codex_lists_the_plugins_its_config_names() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join("config.toml"),
        "model = \"o3\"\n\n[plugins.\"charter@charter\"]\nenabled = true\n\n\
         [plugins.\"deep-research@openai-curated\"]\nenabled = false\n",
    )
    .unwrap();
    let env = env_of(&[("CODEX_HOME", home.path())]);

    let got = CODEX.installed(&Env::of(&env)).expect("it reads");

    assert_eq!(
        got,
        [
            Plugin {
                id: "charter@charter".to_owned(),
                harness: "codex",
                name: "charter".to_owned(),
                source: "charter, on in config.toml".to_owned(),
            },
            Plugin {
                id: "deep-research@openai-curated".to_owned(),
                harness: "codex",
                name: "deep-research".to_owned(),
                source: "openai-curated, off in config.toml".to_owned(),
            },
        ]
    );
}

#[test]
fn opencode_lists_its_config_plugins_and_its_plugin_files() {
    let config = tempfile::tempdir().unwrap();
    let dir = config.path().join("opencode");
    std::fs::create_dir_all(dir.join("plugin")).unwrap();
    std::fs::create_dir_all(dir.join("plugins")).unwrap();
    std::fs::write(
        dir.join("opencode.json"),
        r#"{"plugin": ["opencode-wakatime", "@org/custom@1.2.0"]}"#,
    )
    .unwrap();
    std::fs::write(dir.join("plugin/charter.ts"), "export default {}").unwrap();
    std::fs::write(dir.join("plugins/notes.js"), "export default {}").unwrap();
    std::fs::write(dir.join("plugins/README.md"), "not a plugin").unwrap();
    let env = env_of(&[("XDG_CONFIG_HOME", config.path())]);

    let got = OPENCODE.installed(&Env::of(&env)).expect("it reads");
    let ids: Vec<(&str, &str)> = got
        .iter()
        .map(|it| (it.id.as_str(), it.source.as_str()))
        .collect();

    assert_eq!(
        ids,
        [
            ("@org/custom@1.2.0", "npm, in opencode.json"),
            ("opencode-wakatime", "npm, in opencode.json"),
            ("plugin/charter.ts", "a file in the plugin directory"),
            ("plugins/notes.js", "a file in the plugin directory"),
        ]
    );
}

// -------------------------------------------------------------------------------------
// What a file may not say
// -------------------------------------------------------------------------------------

#[test]
fn a_well_formed_table_is_refused_nothing() {
    assert_eq!(
        refusals(
            "[harness_plugins.claude]\n\"figma@official\" = true\n\n[harness_plugins.codex]\n\"a@b\" = false\n",
            "charter.toml"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_file_is_refused_a_harness_charter_does_not_know_a_value_that_is_not_bool_and_a_pin() {
    let got = refusals(
        "[harness_plugins.vim]\n\"a@b\" = true\n\n[harness_plugins.claude]\n\"figma@official\" = \"yes\"\n\"charter-app@inline\" = false\n",
        "charter.local.toml",
    );
    assert_eq!(
        got,
        [
            "[harness_plugins.vim] in charter.local.toml is not a harness charter knows — one of: \
             claude, opencode, codex"
                .to_owned(),
            "harness_plugins.claude.\"figma@official\" in charter.local.toml is not true or false"
                .to_owned(),
            "harness_plugins.claude.\"charter-app@inline\" in charter.local.toml cannot be false: \
             charter-app@inline is always on: it is charter's own plugin, and it carries \
             charter's hooks and the Bash guard"
                .to_owned(),
        ]
    );
}

#[test]
fn a_harness_plugins_that_is_not_a_table_is_refused() {
    assert_eq!(
        refusals("harness_plugins = 3\n", "charter.toml"),
        [
            "harness_plugins in charter.toml is not a table — each harness is \
          [harness_plugins.<harness>], holding \"<plugin id>\" = true or false"
                .to_owned()
        ]
    );
}

// -------------------------------------------------------------------------------------
// At a chat's start
// -------------------------------------------------------------------------------------

/// A plane whose `charter.toml` is `shared`, and a Claude Code config dir whose install record
/// is `record` — read only through the chat's own `CLAUDE_CONFIG_DIR`.
fn start_with(shared: &str, record: &str) -> Chosen {
    let plane = tempfile::tempdir().unwrap();
    std::fs::write(plane.path().join("charter.toml"), shared).unwrap();
    let config = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(config.path().join("plugins")).unwrap();
    std::fs::write(config.path().join("plugins/installed_plugins.json"), record).unwrap();
    let chat = env_of(&[("CLAUDE_CONFIG_DIR", config.path())]);
    let env = Env {
        chat: &chat,
        home: None,
        process: false,
    };
    for_start("claude", plane.path(), &env)
}

#[test]
fn a_project_that_chooses_nothing_starts_its_chats_with_the_pins_and_reads_no_record() {
    // The record here is not JSON: had it been read, the listing would have failed. It was not,
    // and the answer is the pins, as every chat before this carried.
    assert_eq!(start_with("", "{"), BTreeMap::from(pins(&CLAUDE_CODE)));
}

#[test]
fn a_project_that_chooses_is_handed_what_the_chats_own_record_has_installed() {
    let got = start_with(
        "[harness_plugins.claude]\n\"figma@official\" = false\n\"acme@corp\" = true\n",
        r#"{"version": 2, "plugins": {"figma@official": [{"scope": "user"}]}}"#,
    );
    let mut want = BTreeMap::from(pins(&CLAUDE_CODE));
    want.insert("figma@official".to_owned(), false);
    assert_eq!(got, want);
}

#[test]
fn a_chat_on_a_harness_that_cannot_apply_is_handed_nothing_at_its_start() {
    let plane = tempfile::tempdir().unwrap();
    std::fs::write(
        plane.path().join("charter.toml"),
        "[harness_plugins.codex]\n\"charter@charter\" = false\n",
    )
    .unwrap();
    let env = Env {
        chat: &[],
        home: None,
        process: false,
    };
    assert_eq!(for_start("codex", plane.path(), &env), Chosen::new());
    assert_eq!(for_start("opencode", plane.path(), &env), Chosen::new());
}
