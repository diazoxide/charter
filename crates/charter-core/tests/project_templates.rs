//! Project templates (FR-17, N18): a new project laid out for the stack its repo is in, with
//! personas, a starter `workspace.md`, guard defaults and a `REVIEW.md`, chosen in the first run.

mod support;

use charter_core::firstrun::{self, Choice};
use charter_core::start::{self, Start};
use charter_core::template;

#[test]
fn charter_ships_one_template_per_stack_the_first_run_offers() {
    charter_core::unsteered!();
    let listed: Vec<(String, String)> = template::all()
        .iter()
        .map(|one| (one.id.clone(), one.title.clone()))
        .collect();

    assert_eq!(
        listed,
        [
            ("docs", "Docs only"),
            ("go", "Go"),
            ("monorepo", "Monorepo"),
            ("python", "Python"),
            ("rust", "Rust"),
            ("typescript", "TypeScript"),
        ]
        .map(|(id, title)| (id.to_owned(), title.to_owned()))
    );
}

/// The template detected for a repo holding exactly `files` at its top level.
fn detected(files: &[&str]) -> Option<String> {
    let repo = tempfile::tempdir().expect("a directory");
    for file in files {
        std::fs::write(repo.path().join(file), "").expect("a marker file");
    }
    template::detect(repo.path()).map(|one| one.id.clone())
}

#[test]
fn a_repo_is_the_stack_whose_file_is_at_its_top_level() {
    charter_core::unsteered!();
    assert_eq!(detected(&["Cargo.toml"]).as_deref(), Some("rust"));
    assert_eq!(
        detected(&["package.json", "tsconfig.json"]).as_deref(),
        Some("typescript")
    );
    assert_eq!(detected(&["pyproject.toml"]).as_deref(), Some("python"));
    assert_eq!(detected(&["go.mod"]).as_deref(), Some("go"));
    assert_eq!(detected(&["mkdocs.yml"]).as_deref(), Some("docs"));
}

#[test]
fn a_repo_with_two_stacks_or_a_workspace_file_is_a_monorepo() {
    charter_core::unsteered!();
    assert_eq!(
        detected(&["Cargo.toml", "package.json"]).as_deref(),
        Some("monorepo")
    );
    assert_eq!(
        detected(&["package.json", "pnpm-workspace.yaml"]).as_deref(),
        Some("monorepo")
    );
}

#[test]
fn a_docs_site_inside_a_code_repo_is_the_code_repos_stack() {
    charter_core::unsteered!();
    assert_eq!(detected(&["go.mod", "mkdocs.yml"]).as_deref(), Some("go"));
}

#[test]
fn a_repo_charter_cannot_place_gets_no_template() {
    charter_core::unsteered!();
    assert_eq!(detected(&[]), None);
    assert_eq!(detected(&["README.md"]), None);
}

#[test]
fn a_marker_that_is_a_directory_is_not_the_file_it_is_named_after() {
    charter_core::unsteered!();
    let repo = tempfile::tempdir().expect("a directory");
    std::fs::create_dir(repo.path().join("Cargo.toml")).expect("a directory");

    assert_eq!(template::detect(repo.path()), None);
}

/// A new local project, as the first run makes one, under a config home of its own.
fn new_project() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("a directory");
    let root = charter_core::firstrun::ensure_local_plane(&dir.path().join("config"))
        .expect("a local project");
    (dir, root)
}

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|why| panic!("{}: {why}", path.display()))
}

fn json(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&read(path)).expect("JSON")
}

#[test]
fn a_template_lays_out_its_personas_and_its_review_checklist_in_the_project() {
    charter_core::unsteered!();
    let (_dir, root) = new_project();
    let rust = template::named("rust").expect("the Rust template");

    template::apply(&root, rust).expect("laid out");

    let engineer = read(&root.join("personas/rust-engineer/persona.md"));
    assert!(
        engineer.starts_with("---\nname: rust-engineer\nrole: Rust Engineer\n"),
        "{engineer}"
    );
    let review = read(&root.join("personas/rust-reviewer/refs/REVIEW.md"));
    assert!(
        review.starts_with("# Reviewing a Rust change\n"),
        "{review}"
    );
    for persona in ["rust-engineer", "rust-reviewer"] {
        for kept in ["memory/.gitkeep", "refs/.gitkeep"] {
            assert!(
                root.join("personas").join(persona).join(kept).is_file(),
                "{persona}/{kept}"
            );
        }
    }
    for persona in ["rust-engineer", "rust-reviewer"] {
        let agent = read(&root.join(".claude/agents").join(format!("{persona}.md")));
        assert!(
            agent.contains(charter_core::personaverbs::agents::MARKER),
            "a chat can hand work to {persona}, as to a persona made with `persona create`: {agent}"
        );
    }
    assert!(
        !root.join("template.toml").exists(),
        "the manifest is not copied"
    );
    assert!(
        !root.join("workspace.md").exists(),
        "the starter is a workspace's, not the root's"
    );
    // The front door is still the project's, and still its default.
    assert!(root.join("personas/steward/persona.md").is_file());
    assert!(read(&root.join("charter.toml")).contains("default = \"steward\""));
}

#[test]
fn a_templates_guard_defaults_are_ask_rules_in_every_harness_that_holds_them() {
    charter_core::unsteered!();
    let (_dir, root) = new_project();

    template::apply(&root, template::named("python").expect("Python")).expect("laid out");

    let asks = json(&root.join(".claude/settings.json"))["permissions"]["ask"].clone();
    for rule in [
        "Bash(twine upload *)",
        "Bash(uv publish *)",
        "Bash(charter handoff *)",
    ] {
        assert!(
            asks.as_array().expect("a list").iter().any(|r| r == rule),
            "{rule} in {asks}"
        );
    }
    let opencode = json(&root.join("opencode.json"));
    assert_eq!(
        opencode["permission"]["bash"]["twine upload *"], "ask",
        "{opencode}"
    );
}

#[test]
fn a_persona_the_project_already_has_is_left_whole() {
    charter_core::unsteered!();
    let (_dir, root) = new_project();
    let ours = root.join("personas/go-reviewer");
    std::fs::create_dir_all(&ours).expect("a persona");
    std::fs::write(
        ours.join("persona.md"),
        "---\nname: go-reviewer\n---\nOurs.\n",
    )
    .expect("ours");

    let applied = template::apply(&root, template::named("go").expect("Go")).expect("laid out");

    assert_eq!(
        read(&ours.join("persona.md")),
        "---\nname: go-reviewer\n---\nOurs.\n"
    );
    assert!(
        !ours.join("refs").exists(),
        "nothing is added to a persona somebody has"
    );
    assert!(
        applied
            .written
            .iter()
            .all(|rel| !rel.starts_with("personas/go-reviewer/"))
    );
    assert!(root.join("personas/go-engineer/persona.md").is_file());
}

#[test]
fn applying_a_template_twice_is_applying_it_once() {
    charter_core::unsteered!();
    let (_dir, root) = new_project();
    let ts = template::named("typescript").expect("TypeScript");
    let first = template::apply(&root, ts).expect("laid out");
    let settings = read(&root.join(".claude/settings.json"));

    let again = template::apply(&root, ts).expect("laid out again");

    assert!(
        !first.written.is_empty() && !first.asked.is_empty(),
        "{first:?}"
    );
    assert_eq!(again, template::Applied::default());
    assert_eq!(read(&root.join(".claude/settings.json")), settings);
}

/// A workspace `svc` in the project at `root`, with the `workspace.md` charter gives every one.
fn workspace(root: &std::path::Path) -> std::path::PathBuf {
    let ws = charter_core::workspaces::Plane::open(root)
        .workspace("svc")
        .expect("a workspace name");
    ws.scaffold_charter().expect("its workspace.md");
    ws.dir().join("workspace.md")
}

#[test]
fn a_new_workspace_starts_with_its_templates_context() {
    charter_core::unsteered!();
    let (_dir, root) = new_project();
    let file = workspace(&root);

    let seeded = template::seed_workspace(&root, "svc", template::named("rust").expect("Rust"))
        .expect("seeded");

    let text = read(&file);
    assert!(seeded);
    let context = charter_core::mdsection::section_body(&text, "Context & decisions");
    assert!(
        context.contains("- `cargo clippy --all-targets -- -D warnings`"),
        "{text}"
    );
    assert!(context.contains("`rust-engineer` makes changes"), "{text}");
    assert!(
        text.starts_with("# svc\n"),
        "the rest of the file is charter's own: {text}"
    );
    assert!(
        charter_core::mdsection::section_body(&text, "Vision").starts_with("_Not set yet"),
        "the vision is still the operator's to set: {text}"
    );
}

#[test]
fn a_workspace_whose_context_was_written_keeps_it() {
    charter_core::unsteered!();
    let (_dir, root) = new_project();
    let file = workspace(&root);
    let ours = charter_core::mdsection::replace(
        &read(&file),
        "Context & decisions",
        "We ship on Fridays.",
    );
    std::fs::write(&file, &ours).expect("written");

    let seeded =
        template::seed_workspace(&root, "svc", template::named("go").expect("Go")).expect("asked");

    assert!(!seeded);
    assert_eq!(read(&file), ours);
}

/// A repo with one commit holding `marker` at its top level, beside the project's config home.
fn repo_with(dir: &std::path::Path, name: &str, marker: &str) -> std::path::PathBuf {
    let repo = dir.join(name);
    std::fs::create_dir_all(&repo).expect("the repo's directory");
    support::git(&repo, &["init", "-q", "-b", "main", "."]);
    std::fs::write(repo.join(marker), "").expect("its marker");
    support::git(&repo, &["add", "."]);
    support::git(&repo, &["commit", "-q", "-m", "one"]);
    repo
}

/// A profile `work` declared and approved in the project at `root`, running a stand-in.
fn a_profile(root: &std::path::Path) {
    let bin = stand_in::program(root, "claude-stand-in", "#!/bin/sh\nexit 0\n");
    std::fs::write(
        root.join(charter_core::profiles::LOCAL_FILE),
        format!(
            "[harness.work]\nkind = \"claude\"\ncommand = [{:?}]\n",
            bin.display().to_string()
        ),
    )
    .expect("a profile");
    let set = charter_core::profiles::current(root);
    let profile = set.get("work").expect("declared");
    charter_core::profiletrust::record_launched(
        root,
        "work",
        &charter_core::profiletrust::fingerprint(profile),
    )
    .expect("approved");
}

#[test]
fn a_project_made_from_each_template_starts_a_chat_in_its_repo_under_each_of_its_personas() {
    charter_core::unsteered!();
    let markers = [
        ("docs", "mkdocs.yml"),
        ("go", "go.mod"),
        ("monorepo", "pnpm-workspace.yaml"),
        ("python", "pyproject.toml"),
        ("rust", "Cargo.toml"),
        ("typescript", "tsconfig.json"),
    ];
    assert_eq!(
        markers.len(),
        template::all().len(),
        "every template is started here"
    );
    for (id, marker) in markers {
        let dir = tempfile::tempdir().expect("a directory");
        let root = firstrun::ensure_local_plane(&dir.path().join("config")).expect("a project");
        let repo = repo_with(dir.path(), "widget", marker);
        a_profile(&root);

        let taken = firstrun::take_in_from(&root, &repo, &Choice::Detect).expect("opened");

        assert_eq!(taken.template.as_deref(), Some(id), "{marker}");
        let personas = charter_core::workspaces::Plane::open(&root)
            .personas()
            .expect("listed");
        let theirs: Vec<&String> = personas
            .iter()
            .filter(|p| p.as_str() != "steward")
            .collect();
        assert_eq!(theirs.len(), 2, "{id}: {personas:?}");
        for persona in [None]
            .into_iter()
            .chain(theirs.iter().map(|p| Some(p.to_string())))
        {
            let ready = start::ready(
                &Start {
                    profile: Some("work".to_owned()),
                    persona: persona.clone(),
                    name: "widget 1".to_owned(),
                    cwd: Some(taken.clone.clone()),
                    resume: None,
                    show_footer: false,
                    resuming: None,
                },
                &root,
            );
            assert!(ready.is_ok(), "{id}, {persona:?}: {ready:?}");
        }
    }
}

#[test]
fn a_project_opened_with_no_template_gets_none() {
    charter_core::unsteered!();
    let dir = tempfile::tempdir().expect("a directory");
    let root = firstrun::ensure_local_plane(&dir.path().join("config")).expect("a project");
    let repo = repo_with(dir.path(), "widget", "Cargo.toml");

    let taken = firstrun::take_in_from(&root, &repo, &Choice::Blank).expect("opened");

    assert_eq!(taken.template, None);
    assert!(!root.join("personas/rust-engineer").exists());
}

#[test]
fn the_operators_pick_wins_over_what_the_repo_looks_like() {
    charter_core::unsteered!();
    let dir = tempfile::tempdir().expect("a directory");
    let root = firstrun::ensure_local_plane(&dir.path().join("config")).expect("a project");
    let repo = repo_with(dir.path(), "widget", "Cargo.toml");

    let taken =
        firstrun::take_in_from(&root, &repo, &Choice::Named("docs".to_owned())).expect("opened");

    assert_eq!(taken.template.as_deref(), Some("docs"));
    assert!(root.join("personas/docs-writer/persona.md").is_file());
    let text = read(
        &taken
            .clone
            .parent()
            .expect("the workspace")
            .join("workspace.md"),
    );
    assert!(text.contains("`docs-writer` makes changes"), "{text}");
}

#[test]
fn a_template_charter_does_not_ship_is_refused_before_anything_is_copied() {
    charter_core::unsteered!();
    let dir = tempfile::tempdir().expect("a directory");
    let root = firstrun::ensure_local_plane(&dir.path().join("config")).expect("a project");
    let repo = repo_with(dir.path(), "widget", "Cargo.toml");

    let refused = firstrun::take_in_from(&root, &repo, &Choice::Named("cobol".to_owned()))
        .expect_err("refused");

    assert!(refused.contains("cobol"), "{refused}");
    assert!(!root.join("workspaces/widget").exists());
}

/// Every template's files and what its manifest says beside its version, as one SHA-256 over
/// each in order.
fn digest(one: &template::Template) -> String {
    use sha2::Digest as _;
    let mut hash = sha2::Sha256::new();
    let said = [
        one.title.clone(),
        one.summary.clone(),
        one.detect.join("\n"),
        one.ask.join("\n"),
    ];
    for field in &said {
        hash.update(field.as_bytes());
        hash.update([0]);
    }
    for (rel, text) in &one.files {
        hash.update(rel.as_bytes());
        hash.update([0]);
        hash.update(text.as_bytes());
        hash.update([0]);
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// **A template's version names its files.** A project made from `rust` version 1 got these
/// files and no others, so a change to any of them is a new version: bump `version` in the
/// template's `template.toml` and put the new version and digest here, in the same commit.
#[test]
fn a_templates_files_change_only_with_its_version() {
    charter_core::unsteered!();
    let published: [(&str, u32, &str); 6] = [
        (
            "docs",
            1,
            "6f92ee1afc213d2e4f03f6cadd9635f3121774eb75d4d03c1ba867a592589ad8",
        ),
        (
            "go",
            1,
            "f2bdcf8405841e286b0873dbc364169b1e0413778308310ed8a0c4b4c53897f3",
        ),
        (
            "monorepo",
            1,
            "676bf89cd7f1d898bf42cb6183d92f2c45c05da7b6d2be7e4c7e713c228900e5",
        ),
        (
            "python",
            1,
            "e25d238ce449260fa58117357b3a924ed679c20f329f9c7382dbdb8e8d2cdd43",
        ),
        (
            "rust",
            1,
            "92fc0b456a6c9fda90405b93f6cee4f5ba54744f15ec1135be528ce5ed65ce59",
        ),
        (
            "typescript",
            1,
            "30e796f733f0c2f36360091ab2b95ddd3d5a96974ce19f88be35fc4724d5a6c7",
        ),
    ];
    let now: Vec<(String, u32, String)> = template::all()
        .iter()
        .map(|one| (one.id.clone(), one.version, digest(one)))
        .collect();

    assert_eq!(
        now,
        published.map(|(id, version, digest)| (id.to_owned(), version, digest.to_owned())),
        "a template's files changed without its version: bump `version` in its template.toml, \
         and record the new version and digest here"
    );
}
