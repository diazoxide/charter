use std::path::{Path, PathBuf};

use super::*;
use crate::personaverbs::lint::Level;

/// A plane with `charter.toml`, the named personas (each with a finished definition) and the
/// named workspaces. `default` is written as `[persona] default` when given.
fn plane(personas: &[&str], workspaces: &[&str], default: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let toml = match default {
        Some(name) => format!("[persona]\ndefault = \"{name}\"\n"),
        None => String::new(),
    };
    std::fs::write(dir.path().join("charter.toml"), toml).unwrap();
    for name in personas {
        let d = dir.path().join("personas").join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("persona.md"),
            format!("---\nname: {name}\nrole: R\nvault: none\ndelegate-when: w\n---\n\n# {name}\n"),
        )
        .unwrap();
    }
    for ws in workspaces {
        let d = dir.path().join("workspaces").join(ws);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("workspace.md"), "# ws\n").unwrap();
    }
    dir
}

fn declare(root: &Path, persona: &str, id: &str, text: &str) -> PathBuf {
    let d = root.join("personas").join(persona).join("curation");
    std::fs::create_dir_all(&d).unwrap();
    let path = d.join(format!("{id}.md"));
    std::fs::write(&path, text).unwrap();
    path
}

const REVIEW: &str =
    "---\nlabel: Review it\non: workspace, persona\n---\n\nReview {subject.kind} {subject.name}.\n";

fn errors(parsed: &Parsed) -> Vec<String> {
    parsed
        .issues
        .iter()
        .filter(|i| i.level == Level::Error)
        .map(|i| i.message.clone())
        .collect()
}

fn warnings(parsed: &Parsed) -> Vec<String> {
    parsed
        .issues
        .iter()
        .filter(|i| i.level == Level::Warn)
        .map(|i| i.message.clone())
        .collect()
}

// ------------------------------------------------------------------------------------------
// Parsing one file

#[test]
fn a_file_gives_its_label_its_kinds_where_it_runs_and_its_template() {
    let parsed = parse(
        "ops",
        "review",
        "---\nlabel: Review it\non: [workspace, plane]\nruns-in: plane\n---\n\nReview {subject.name}.\n",
    );
    assert!(parsed.issues.is_empty(), "{:?}", parsed.issues);
    let action = parsed.action.expect("an action");
    assert_eq!(action.persona, "ops");
    assert_eq!(action.id, "review");
    assert_eq!(action.label, "Review it");
    assert_eq!(action.on, vec![Kind::Workspace, Kind::Plane]);
    assert_eq!(action.runs_in, Some(RunsIn::Plane));
    assert_eq!(action.template, "Review {subject.name}.");
}

#[test]
fn a_file_with_no_label_is_an_error_and_not_an_action() {
    let parsed = parse("ops", "x", "---\non: workspace\n---\n\nDo it.\n");
    assert!(parsed.action.is_none());
    assert_eq!(
        errors(&parsed),
        vec!["no label: — the menu has nothing to show"]
    );
}

#[test]
fn a_file_offered_on_nothing_is_an_error() {
    let missing = parse("ops", "x", "---\nlabel: X\n---\n\nDo it.\n");
    assert!(missing.action.is_none());
    assert_eq!(missing.on, None, "no on: means the kinds are unknown");
    assert_eq!(
        errors(&missing),
        vec!["no on: — name the subjects it is offered on: workspace, persona, plane"]
    );
    let empty = parse("ops", "x", "---\nlabel: X\non: []\n---\n\nDo it.\n");
    assert!(empty.action.is_none());
}

#[test]
fn an_unknown_kind_of_subject_is_an_error_naming_it() {
    let parsed = parse(
        "ops",
        "x",
        "---\nlabel: X\non: workspace, repo\n---\n\nDo it.\n",
    );
    assert!(parsed.action.is_none());
    assert_eq!(
        errors(&parsed),
        vec!["on: 'repo' is not a kind of subject — workspace, persona, plane"]
    );
    assert_eq!(parsed.on, Some(vec![Kind::Workspace]));
}

#[test]
fn an_unknown_runs_in_is_an_error_rather_than_a_guess() {
    let parsed = parse(
        "ops",
        "x",
        "---\nlabel: X\non: workspace\nruns-in: repo\n---\n\nDo it.\n",
    );
    assert!(parsed.action.is_none());
    assert_eq!(
        errors(&parsed),
        vec!["runs-in: 'repo' is neither subject nor plane"]
    );
}

#[test]
fn an_unknown_template_variable_is_an_error_naming_it() {
    let parsed = parse(
        "ops",
        "x",
        "---\nlabel: X\non: workspace\n---\n\nIn {subject.path} run {vault.token} and {env.HOME}.\n",
    );
    assert!(parsed.action.is_none());
    assert_eq!(
        errors(&parsed),
        vec![
            "the prompt uses {vault.token}, which is not a variable — only {subject.kind}, \
             {subject.name}, {subject.path} and {plane.root} are",
            "the prompt uses {env.HOME}, which is not a variable — only {subject.kind}, \
             {subject.name}, {subject.path} and {plane.root} are",
        ]
    );
}

#[test]
fn braces_around_anything_but_a_word_are_text() {
    let parsed = parse(
        "ops",
        "x",
        "---\nlabel: X\non: workspace\n---\n\nSend {\"a\": 1} and { spaced } and {}.\n",
    );
    assert!(parsed.issues.is_empty(), "{:?}", parsed.issues);
}

#[test]
fn an_empty_prompt_is_an_error() {
    let parsed = parse("ops", "x", "---\nlabel: X\non: workspace\n---\n\n   \n");
    assert!(parsed.action.is_none());
    assert_eq!(
        errors(&parsed),
        vec!["the prompt is empty — write it below the frontmatter"]
    );
}

#[test]
fn an_unknown_key_is_a_warning_and_the_action_still_counts() {
    let parsed = parse(
        "ops",
        "x",
        "---\nlabel: X\non: workspace\ncolour: red\n---\n\nDo it.\n",
    );
    assert!(parsed.action.is_some());
    assert_eq!(
        warnings(&parsed),
        vec!["unknown key 'colour' — charter reads label, on and runs-in"]
    );
}

#[test]
fn a_repeated_key_is_an_error() {
    let parsed = parse(
        "ops",
        "x",
        "---\nlabel: X\nlabel: Y\non: workspace\n---\n\nDo it.\n",
    );
    assert!(parsed.action.is_none());
    assert_eq!(errors(&parsed), vec!["label: is given twice"]);
}

#[test]
fn an_id_charter_could_not_mint_is_an_error() {
    let parsed = parse("ops", "Bad Id", REVIEW);
    assert!(parsed.action.is_none());
    assert_eq!(
        errors(&parsed),
        vec!["'Bad Id' is not an action id — lowercase letters, digits, '.', '_' and '-'"]
    );
}

#[test]
fn a_file_whose_id_is_a_builtins_is_refused_because_a_builtin_cannot_be_overridden() {
    let parsed = parse("ops", "safe-remove", REVIEW);
    assert!(parsed.action.is_none());
    assert_eq!(
        errors(&parsed),
        vec![
            "the id 'safe-remove' is charter's own (charter/safe-remove), which a persona \
              cannot override — rename the file"
        ]
    );
}

#[test]
fn a_file_whose_label_is_a_builtins_in_any_case_is_refused() {
    let parsed = parse(
        "ops",
        "tidy",
        "---\nlabel:  compact & IMPROVE \non: workspace\n---\n\nDo it.\n",
    );
    assert!(parsed.action.is_none());
    assert_eq!(
        errors(&parsed),
        vec![
            "the label 'compact & IMPROVE' is charter's own (charter/compact), which a \
              persona cannot take — choose another"
        ]
    );
}

// ------------------------------------------------------------------------------------------
// Rendering

fn vars() -> Vars {
    Vars {
        kind: Kind::Workspace,
        name: "web".into(),
        path: "/p/workspaces/{plane.root}".into(),
        plane_root: "/p".into(),
    }
}

#[test]
fn rendering_substitutes_the_four_variables() {
    let out = render(
        "{subject.kind} {subject.name} at {subject.path} in {plane.root}.",
        &vars(),
    )
    .unwrap();
    assert_eq!(out, "workspace web at /p/workspaces/{plane.root} in /p.");
}

#[test]
fn a_substituted_value_is_never_read_again_for_variables() {
    // The workspace path above holds `{plane.root}` literally, and it stays literal.
    let out = render("{subject.path}", &vars()).unwrap();
    assert_eq!(out, "/p/workspaces/{plane.root}");
}

#[test]
fn rendering_leaves_shell_syntax_alone() {
    let out = render("echo $HOME $(whoami) `id` {subject.name}", &vars()).unwrap();
    assert_eq!(out, "echo $HOME $(whoami) `id` web");
}

#[test]
fn a_shell_style_brace_variable_is_an_unknown_variable_not_an_expansion() {
    // `${USER}` holds `{USER}`, a word in braces: refused like any other unknown variable,
    // so it can never reach a chat looking as if it had been filled in.
    assert_eq!(render("hi ${USER}", &vars()), Err(vec!["USER".to_string()]));
}

#[test]
fn rendering_refuses_an_unknown_variable() {
    assert_eq!(
        render("hi {subject.owner}", &vars()),
        Err(vec!["subject.owner".to_string()])
    );
}

// ------------------------------------------------------------------------------------------
// Subjects

#[test]
fn a_subject_is_read_as_kind_colon_name_or_the_word_plane() {
    assert_eq!(
        Subject::parse("workspace:web"),
        Ok(Subject {
            kind: Kind::Workspace,
            name: Some("web".into())
        })
    );
    assert_eq!(
        Subject::parse("persona:ops"),
        Ok(Subject {
            kind: Kind::Persona,
            name: Some("ops".into())
        })
    );
    assert_eq!(
        Subject::parse("plane"),
        Ok(Subject {
            kind: Kind::Plane,
            name: None
        })
    );
    assert!(Subject::parse("workspace").is_err());
    assert!(Subject::parse("workspace:").is_err());
    assert!(Subject::parse("repo:x").is_err());
    assert!(Subject::parse("plane:x").is_err());
}

// ------------------------------------------------------------------------------------------
// Resolving what a subject is offered

fn ids(resolution: &Resolution) -> Vec<&str> {
    resolution.actions.iter().map(|a| a.id.as_str()).collect()
}

fn ws(name: &str) -> Subject {
    Subject {
        kind: Kind::Workspace,
        name: Some(name.into()),
    }
}

fn persona(name: &str) -> Subject {
    Subject {
        kind: Kind::Persona,
        name: Some(name.into()),
    }
}

#[test]
fn charters_own_come_first_then_each_personas_by_persona_then_id() {
    let dir = plane(&["ops", "ada"], &["web"], None);
    let root = dir.path();
    declare(root, "ops", "zeta", REVIEW);
    declare(
        root,
        "ops",
        "alpha",
        REVIEW.replace("Review it", "Alpha").as_str(),
    );
    declare(
        root,
        "ada",
        "beta",
        REVIEW.replace("Review it", "Beta").as_str(),
    );
    let got = resolve(root, &ws("web")).unwrap();
    assert_eq!(
        ids(&got),
        vec![
            "charter/safe-remove",
            "charter/compact",
            "ada/beta",
            "ops/alpha",
            "ops/zeta"
        ]
    );
    assert!(got.warnings.is_empty(), "{:?}", got.warnings);
    assert_eq!(got.actions[0].label, "Safe remove");
    assert_eq!(got.actions[1].label, "Compact & improve");
    assert_eq!(got.actions[0].source, Source::Charter);
    assert_eq!(got.actions[2].source, Source::Persona("ada".into()));
}

#[test]
fn only_actions_offered_on_the_subjects_kind_are_listed() {
    let dir = plane(&["ops"], &["web"], None);
    let root = dir.path();
    declare(root, "ops", "p", "---\nlabel: P\non: persona\n---\n\nP.\n");
    declare(root, "ops", "pl", "---\nlabel: Pl\non: plane\n---\n\nPl.\n");
    assert_eq!(
        ids(&resolve(root, &ws("web")).unwrap()),
        vec!["charter/safe-remove", "charter/compact"]
    );
    assert_eq!(
        ids(&resolve(root, &persona("ops")).unwrap()),
        vec![
            "charter/safe-remove",
            "charter/compact",
            "charter/add-curation-action",
            "ops/p"
        ]
    );
    let on_plane = Subject {
        kind: Kind::Plane,
        name: None,
    };
    assert_eq!(ids(&resolve(root, &on_plane).unwrap()), vec!["ops/pl"]);
}

#[test]
fn a_clashing_file_is_dropped_with_a_warning_that_names_it() {
    let dir = plane(&["ops"], &["web"], None);
    let root = dir.path();
    declare(root, "ops", "safe-remove", REVIEW);
    let got = resolve(root, &ws("web")).unwrap();
    assert_eq!(ids(&got), vec!["charter/safe-remove", "charter/compact"]);
    assert_eq!(
        got.warnings,
        vec![
            "personas/ops/curation/safe-remove.md is not offered: the id 'safe-remove' is \
              charter's own (charter/safe-remove), which a persona cannot override — rename \
              the file. `charter persona lint ops` lists every problem"
        ]
    );
}

#[test]
fn a_broken_file_whose_kinds_are_unknown_is_warned_about_on_every_subject() {
    let dir = plane(&["ops"], &["web"], None);
    let root = dir.path();
    declare(root, "ops", "x", "---\nlabel: X\n---\n\nX.\n");
    declare(
        root,
        "ops",
        "y",
        "---\nlabel: Y\non: persona\nruns-in: nowhere\n---\n\nY.\n",
    );
    let got = resolve(root, &ws("web")).unwrap();
    // `x` could be for anything, so it is named; `y` is for personas, so a workspace's list
    // does not carry its problem.
    assert_eq!(got.warnings.len(), 1, "{:?}", got.warnings);
    assert!(got.warnings[0].starts_with("personas/ops/curation/x.md is not offered: no on:"));
}

#[test]
fn a_persona_action_is_run_by_the_persona_that_declares_it() {
    let dir = plane(&["ops", "ada"], &["web"], Some("ada"));
    let root = dir.path();
    declare(root, "ops", "review", REVIEW);
    let got = resolve(root, &persona("ada")).unwrap();
    let action = got.actions.iter().find(|a| a.id == "ops/review").unwrap();
    assert_eq!(action.runner.as_deref(), Some("ops"));
}

#[test]
fn charters_own_on_a_persona_are_run_by_that_persona_itself() {
    let dir = plane(&["ops", "ada"], &[], Some("ada"));
    let got = resolve(dir.path(), &persona("ops")).unwrap();
    for action in &got.actions {
        assert_eq!(action.runner.as_deref(), Some("ops"), "{}", action.id);
    }
}

#[test]
fn charters_own_on_a_workspace_are_run_by_the_planes_default_persona() {
    let dir = plane(&["ops", "ada"], &["web"], Some("ada"));
    let got = resolve(dir.path(), &ws("web")).unwrap();
    for action in &got.actions {
        assert_eq!(action.runner.as_deref(), Some("ada"), "{}", action.id);
    }
}

#[test]
fn charters_own_on_a_workspace_run_as_no_persona_when_the_plane_names_no_default() {
    let dir = plane(&["ops"], &["web"], None);
    let got = resolve(dir.path(), &ws("web")).unwrap();
    for action in &got.actions {
        assert_eq!(action.runner, None, "{}", action.id);
    }
}

#[test]
fn a_default_naming_a_persona_that_is_gone_is_no_persona() {
    let dir = plane(&["ops"], &["web"], Some("gone"));
    let got = resolve(dir.path(), &ws("web")).unwrap();
    assert_eq!(got.actions[0].runner, None);
}

fn cwd_of(resolution: &Resolution, id: &str) -> PathBuf {
    resolution
        .actions
        .iter()
        .find(|a| a.id == id)
        .unwrap_or_else(|| panic!("{id} in {:?}", ids(resolution)))
        .cwd
        .clone()
}

#[test]
fn a_workspace_action_runs_in_the_workspace_unless_it_says_plane() {
    let dir = plane(&["ops"], &["web"], None);
    let root = dir.path();
    declare(
        root,
        "ops",
        "here",
        "---\nlabel: Here\non: workspace\n---\n\nH.\n",
    );
    declare(
        root,
        "ops",
        "up",
        "---\nlabel: Up\non: workspace\nruns-in: plane\n---\n\nU.\n",
    );
    let got = resolve(root, &ws("web")).unwrap();
    assert_eq!(cwd_of(&got, "ops/here"), root.join("workspaces/web"));
    assert_eq!(cwd_of(&got, "ops/up"), root.to_path_buf());
}

#[test]
fn a_persona_action_runs_at_the_plane_root_unless_it_says_subject() {
    let dir = plane(&["ops"], &[], None);
    let root = dir.path();
    declare(
        root,
        "ops",
        "here",
        "---\nlabel: Here\non: persona\n---\n\nH.\n",
    );
    declare(
        root,
        "ops",
        "in",
        "---\nlabel: In\non: persona\nruns-in: subject\n---\n\nI.\n",
    );
    let got = resolve(root, &persona("ops")).unwrap();
    assert_eq!(cwd_of(&got, "ops/here"), root.to_path_buf());
    assert_eq!(cwd_of(&got, "ops/in"), root.join("personas/ops"));
}

#[test]
fn a_plane_action_runs_at_the_plane_root_either_way() {
    let dir = plane(&["ops"], &[], None);
    let root = dir.path();
    declare(
        root,
        "ops",
        "a",
        "---\nlabel: A\non: plane\nruns-in: subject\n---\n\nA.\n",
    );
    let on_plane = Subject {
        kind: Kind::Plane,
        name: None,
    };
    let got = resolve(root, &on_plane).unwrap();
    assert_eq!(cwd_of(&got, "ops/a"), root.to_path_buf());
}

#[test]
fn safe_remove_runs_at_the_plane_root_because_its_subject_is_going_away() {
    let dir = plane(&["ops"], &["web"], None);
    let root = dir.path();
    let got = resolve(root, &ws("web")).unwrap();
    assert_eq!(cwd_of(&got, "charter/safe-remove"), root.to_path_buf());
    assert_eq!(cwd_of(&got, "charter/compact"), root.join("workspaces/web"));
}

#[test]
fn the_prompt_is_rendered_for_the_subject() {
    let dir = plane(&["ops"], &["web"], None);
    let root = dir.path();
    declare(
        root,
        "ops",
        "review",
        "---\nlabel: R\non: workspace\n---\n\n{subject.kind}|{subject.name}|{subject.path}|{plane.root}\n",
    );
    let got = resolve(root, &ws("web")).unwrap();
    let action = got.actions.iter().find(|a| a.id == "ops/review").unwrap();
    assert_eq!(
        action.prompt,
        format!(
            "workspace|web|{}|{}",
            root.join("workspaces/web").display(),
            root.display()
        )
    );
}

#[test]
fn a_subject_that_is_not_there_is_refused() {
    let dir = plane(&["ops"], &["web"], None);
    let root = dir.path();
    assert_eq!(
        resolve(root, &ws("nope")).unwrap_err(),
        "no workspace 'nope'"
    );
    assert_eq!(
        resolve(root, &persona("nope")).unwrap_err(),
        "no persona 'nope'"
    );
    assert_eq!(
        resolve(root, &ws("../web")).unwrap_err(),
        "no workspace '../web'"
    );
}

// ------------------------------------------------------------------------------------------
// charter's own

#[test]
fn each_builtin_prompt_names_its_skill_uses_only_known_variables_and_no_slash_command() {
    for builtin in BUILTINS {
        for kind in builtin.on {
            let template = (builtin.template)(*kind);
            assert!(
                template_problems(template).is_empty(),
                "{} on {kind:?}: {:?}",
                builtin.id,
                template_problems(template)
            );
            assert!(
                template.contains(&format!("`{}` skill", builtin.skill)),
                "{} on {kind:?} does not name its skill",
                builtin.id
            );
            assert!(
                !template.lines().any(|l| l.trim_start().starts_with('/')),
                "{} on {kind:?} carries a slash command",
                builtin.id
            );
        }
    }
}

#[test]
fn each_builtin_names_a_skill_charters_plugin_ships() {
    let skills = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src-tauri/plugin/skills");
    for builtin in BUILTINS {
        let skill = skills.join(builtin.skill).join("SKILL.md");
        let text =
            std::fs::read_to_string(&skill).unwrap_or_else(|e| panic!("{}: {e}", skill.display()));
        assert!(
            text.starts_with(&format!("---\nname: {}\n", builtin.skill)),
            "{} does not declare its name",
            skill.display()
        );
    }
}

// ------------------------------------------------------------------------------------------
// Declaring and removing

#[test]
fn adding_an_action_writes_a_file_that_reads_back_as_the_same_action() {
    let dir = plane(&["ops"], &["web"], None);
    let root = dir.path();
    let path = add(
        root,
        &New {
            persona: "ops",
            id: "review",
            label: "Review it",
            on: &[Kind::Workspace, Kind::Persona],
            runs_in: Some(RunsIn::Plane),
            prompt: "Review {subject.name}.\n",
        },
    )
    .unwrap();
    assert_eq!(path, root.join("personas/ops/curation/review.md"));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "---\nlabel: Review it\non: workspace, persona\nruns-in: plane\n---\n\nReview {subject.name}.\n"
    );
    let found = declared(root, "ops");
    assert_eq!(found.len(), 1);
    assert!(found[0].issues.is_empty(), "{:?}", found[0].issues);
}

#[test]
fn adding_refuses_an_action_that_would_not_read_back_and_writes_nothing() {
    let dir = plane(&["ops"], &[], None);
    let root = dir.path();
    let refused = add(
        root,
        &New {
            persona: "ops",
            id: "bad",
            label: "Bad",
            on: &[Kind::Workspace],
            runs_in: None,
            prompt: "Read {secret.x}",
        },
    )
    .unwrap_err();
    assert!(refused.contains("{secret.x}"), "{refused}");
    assert!(!root.join("personas/ops/curation/bad.md").exists());
}

#[test]
fn adding_never_writes_over_a_file_that_is_there() {
    let dir = plane(&["ops"], &[], None);
    let root = dir.path();
    let path = declare(root, "ops", "review", "mine\n");
    let refused = add(
        root,
        &New {
            persona: "ops",
            id: "review",
            label: "R",
            on: &[Kind::Workspace],
            runs_in: None,
            prompt: "P",
        },
    )
    .unwrap_err();
    assert_eq!(
        refused,
        "personas/ops/curation/review.md already exists — remove it first: charter persona \
         curation remove ops review"
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), "mine\n");
}

#[test]
fn adding_to_a_persona_that_is_not_there_is_refused() {
    let dir = plane(&["ops"], &[], None);
    let refused = add(
        dir.path(),
        &New {
            persona: "nope",
            id: "r",
            label: "R",
            on: &[Kind::Workspace],
            runs_in: None,
            prompt: "P",
        },
    )
    .unwrap_err();
    assert_eq!(refused, "no persona 'nope'");
}

#[test]
fn a_label_on_two_lines_is_refused() {
    let dir = plane(&["ops"], &[], None);
    let refused = add(
        dir.path(),
        &New {
            persona: "ops",
            id: "r",
            label: "R\nlabel: S",
            on: &[Kind::Workspace],
            runs_in: None,
            prompt: "P",
        },
    )
    .unwrap_err();
    assert_eq!(refused, "a label is one line");
}

#[test]
fn removing_an_action_deletes_its_file() {
    let dir = plane(&["ops"], &[], None);
    let root = dir.path();
    let path = declare(root, "ops", "review", REVIEW);
    assert_eq!(remove(root, "ops", "review").unwrap(), path);
    assert!(!path.exists());
    assert_eq!(
        remove(root, "ops", "review").unwrap_err(),
        "ops has no curation action 'review'"
    );
}

#[test]
fn only_markdown_files_directly_in_curation_are_actions() {
    let dir = plane(&["ops"], &[], None);
    let root = dir.path();
    declare(root, "ops", "review", REVIEW);
    let d = root.join("personas/ops/curation");
    std::fs::write(d.join(".gitkeep"), "").unwrap();
    std::fs::write(d.join("notes.txt"), "x").unwrap();
    std::fs::create_dir_all(d.join("sub")).unwrap();
    std::fs::write(d.join("sub/inner.md"), REVIEW).unwrap();
    let found = declared(root, "ops");
    assert_eq!(
        found.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        vec!["review"]
    );
}

#[cfg(unix)]
#[test]
fn a_file_that_links_out_of_the_plane_is_an_error_not_an_action() {
    let dir = plane(&["ops"], &[], None);
    let root = dir.path();
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside.path(), REVIEW).unwrap();
    let d = root.join("personas/ops/curation");
    std::fs::create_dir_all(&d).unwrap();
    std::os::unix::fs::symlink(outside.path(), d.join("leak.md")).unwrap();
    let found = declared(root, "ops");
    assert_eq!(found.len(), 1);
    assert!(found[0].action.is_none());
    assert_eq!(
        errors(&found[0]),
        vec![
            "charter will not read it (outside the plane, not a regular file, too large, or not UTF-8)"
        ]
    );
}

// ------------------------------------------------------------------------------------------
// `charter persona lint` carries the same findings

#[test]
fn persona_lint_reports_each_curation_problem_under_its_file() {
    let dir = plane(&["ops"], &[], None);
    let root = dir.path();
    declare(root, "ops", "safe-remove", REVIEW);
    declare(
        root,
        "ops",
        "ok",
        "---\nlabel: Ok\non: plane\nhue: x\n---\n\nFine.\n",
    );
    let state = root.join(".charter");
    let linter = crate::personaverbs::lint::Linter::new(root, &state).with_home(None);
    let issues: Vec<(Level, String)> = linter
        .definition("ops")
        .into_iter()
        .filter(|i| i.message.starts_with("curation/"))
        .map(|i| (i.level, i.message))
        .collect();
    assert_eq!(
        issues,
        vec![
            (
                Level::Warn,
                "curation/ok.md: unknown key 'hue' — charter reads label, on and runs-in".into()
            ),
            (
                Level::Error,
                "curation/safe-remove.md: the id 'safe-remove' is charter's own \
                 (charter/safe-remove), which a persona cannot override — rename the file"
                    .into()
            ),
        ]
    );
}
