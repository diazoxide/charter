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
    let root = charter_core::firstrun::ensure_local_plane(
        &dir.path().join("config"),
        charter_core::firstrun::ForgeFrom::Named(charter_core::forge::Kind::GitHub),
    )
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

    template::apply(&root, rust, None).expect("laid out");

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
            charter_core::personaverbs::agents::carries_marker(&agent),
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

    template::apply(&root, template::named("python").expect("Python"), None).expect("laid out");

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

    let applied =
        template::apply(&root, template::named("go").expect("Go"), None).expect("laid out");

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
    let first = template::apply(&root, ts, None).expect("laid out");
    let settings = read(&root.join(".claude/settings.json"));

    let again = template::apply(&root, ts, None).expect("laid out again");

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
/// A `work` profile on a Claude Code stand-in, approved. The script is in `outside`, a folder
/// no chat writes, run by `/bin/sh`, and answers `--version` as Claude Code does: a project charter makes runs its chats
/// sandboxed (ADR 0067 §1), and a sandbox binds only that, never a file inside the project
/// (ruling V87g).
fn a_profile(root: &std::path::Path, outside: &std::path::Path) {
    let bin = stand_in::program(
        outside,
        "claude-stand-in",
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo \"2.1.288 (Claude Code)\"; fi\nexit 0\n",
    );
    // Run as `/bin/sh <script>`, the script in a folder no chat writes.
    std::fs::write(
        root.join(charter_core::profiles::LOCAL_FILE),
        format!(
            "[harness.work]\nkind = \"claude\"\ncommand = [\"/bin/sh\", {:?}]\n",
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
        let root = firstrun::ensure_local_plane(
            &dir.path().join("config"),
            charter_core::firstrun::ForgeFrom::Named(charter_core::forge::Kind::GitHub),
        )
        .expect("a project");
        let repo = repo_with(dir.path(), "widget", marker);
        // Where no chat writes: a sandboxed start refuses a program, or any file its command
        // names, in the project or a temp folder (ruling V87g, D-88g).
        let bin = stand_in::NoChatWrites::new();
        a_profile(&root, bin.path());

        let taken = firstrun::take_in_from(&root, &repo, &Choice::Fits).expect("opened");

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
                    without_sandbox: None,
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
    let root = firstrun::ensure_local_plane(
        &dir.path().join("config"),
        charter_core::firstrun::ForgeFrom::Named(charter_core::forge::Kind::GitHub),
    )
    .expect("a project");
    let repo = repo_with(dir.path(), "widget", "Cargo.toml");

    let taken = firstrun::take_in_from(&root, &repo, &Choice::NoTemplate).expect("opened");

    assert_eq!(taken.template, None);
    assert!(!root.join("personas/rust-engineer").exists());
}

#[test]
fn the_operators_pick_wins_over_what_the_repo_looks_like() {
    charter_core::unsteered!();
    let dir = tempfile::tempdir().expect("a directory");
    let root = firstrun::ensure_local_plane(
        &dir.path().join("config"),
        charter_core::firstrun::ForgeFrom::Named(charter_core::forge::Kind::GitHub),
    )
    .expect("a project");
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
    let root = firstrun::ensure_local_plane(
        &dir.path().join("config"),
        charter_core::firstrun::ForgeFrom::Named(charter_core::forge::Kind::GitHub),
    )
    .expect("a project");
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
        format!("{:?}", one.kind),
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

/// Every version of every template charter has published, with the digest of its files and
/// manifest. **A row is never edited and never removed.** A project made from `rust` version 1
/// got exactly the files that row names, so a change to any of them is version 2: bump
/// `version` in the template's `template.toml` and add the new row below the old one.
const PUBLISHED: &[(&str, u32, &str)] = &[
    (
        "docs",
        1,
        "8bd2a0aa7d53f576638d8b2414fc55e85fdad45cd584a85901a44de4f92299d2",
    ),
    (
        "go",
        1,
        "79c5d42f57be5ffb3ad7afaca77e8419d05455512b3e87fa09a4305bc591227a",
    ),
    (
        "monorepo",
        1,
        "d198a336c66bfa6089a2cb3aa97c5b332f012fb8bb77231c774b1bb46f9f0224",
    ),
    (
        "python",
        1,
        "7d7062de4335df3c0656aaf6a478706d54a80ddb9f680f9b165cb5e949f1a803",
    ),
    (
        "rust",
        1,
        "06d59b47e2ffd6b903cf50e84e88762c4d89f82053a81b14172770480b8c0f62",
    ),
    (
        "typescript",
        1,
        "6377d824518eda64c706db056d31ed2101ccf859a6b76b2fec83b7b69ddcec75",
    ),
];

#[test]
fn a_published_template_version_never_changes() {
    charter_core::unsteered!();
    for one in template::all() {
        let now = digest(one);
        let rows: Vec<&(&str, u32, &str)> = PUBLISHED
            .iter()
            .filter(|(id, _, _)| *id == one.id)
            .collect();
        match rows.iter().find(|(_, version, _)| *version == one.version) {
            Some((_, version, was)) => assert_eq!(
                *was,
                now,
                "{} version {version} was published with other files. A published version \
                 never changes: put this change in version {} of {}'s template.toml, and add \
                 that version's row to PUBLISHED below this one, which stays as it is",
                one.id,
                version + 1,
                one.id
            ),
            None => panic!(
                "{} version {} has no row in PUBLISHED. Add (\"{}\", {}, \"{now}\") after its \
                 last row",
                one.id, one.version, one.id, one.version
            ),
        }
        let newest = rows.iter().map(|(_, version, _)| *version).max();
        assert_eq!(
            newest,
            Some(one.version),
            "{}'s template.toml says version {}, and PUBLISHED records a newer one: a version \
             never goes back",
            one.id,
            one.version
        );
    }
    for (id, _, _) in PUBLISHED {
        assert!(
            template::named(id).is_some(),
            "PUBLISHED names {id}, which charter no longer ships"
        );
    }
}

#[test]
fn a_monorepo_asks_about_every_command_any_stack_in_it_asks_about() {
    charter_core::unsteered!();
    let monorepo = template::named("monorepo").expect("Monorepo");
    for stack in template::all()
        .iter()
        .filter(|one| one.kind == template::Kind::Stack)
    {
        for rule in &stack.ask {
            assert!(
                monorepo.ask.contains(rule),
                "{} asks about {rule}",
                stack.id
            );
        }
    }
    assert!(
        monorepo.ask.contains(&"changeset publish *".to_owned()),
        "and its own"
    );
}

#[test]
fn every_docs_site_charter_detects_has_its_deploy_asked_about() {
    charter_core::unsteered!();
    let docs = template::named("docs").expect("Docs only");

    assert!(docs.ask.contains(&"mkdocs gh-deploy *".to_owned()));
    assert!(
        docs.ask.contains(&"docusaurus deploy *".to_owned()),
        "{:?}",
        docs.ask
    );
    assert_eq!(
        detected(&["antora.yml"]),
        None,
        "no Antora deploy to ask about, so not offered"
    );
}

#[cfg(unix)]
#[test]
fn a_marker_that_is_a_link_does_not_count() {
    charter_core::unsteered!();
    let repo = tempfile::tempdir().expect("a directory");
    let elsewhere = tempfile::tempdir().expect("a directory");
    std::fs::write(elsewhere.path().join("Cargo.toml"), "").expect("a file");
    std::os::unix::fs::symlink(
        elsewhere.path().join("Cargo.toml"),
        repo.path().join("Cargo.toml"),
    )
    .expect("a link");

    assert_eq!(template::detect(repo.path()), None);
}

/// The `- ` lines of `text` under `## <header>`, in order.
fn bullets(text: &str, header: &str) -> Vec<String> {
    charter_core::mdsection::section_body(text, header)
        .lines()
        .filter_map(|line| line.strip_prefix("- "))
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_workspace_is_told_the_same_checks_its_templates_engineer_runs() {
    charter_core::unsteered!();
    for one in template::all() {
        let file = |rel: &str| {
            one.files
                .iter()
                .find(|(path, _)| path == rel)
                .map(|(_, text)| *text)
                .unwrap_or_else(|| panic!("{} has no {rel}", one.id))
        };
        let engineer = one
            .files
            .iter()
            .find(|(path, text)| {
                path.ends_with("/persona.md") && !path.contains("-reviewer/") && !text.is_empty()
            })
            .map(|(_, text)| *text)
            .unwrap_or_else(|| panic!("{} has no engineer", one.id));

        let runs = bullets(engineer, "How a change is checked");
        assert!(!runs.is_empty(), "{}", one.id);
        assert_eq!(
            bullets(file("workspace.md"), "Context & decisions"),
            runs,
            "{}",
            one.id
        );
    }
}

#[test]
fn a_template_a_harness_file_refuses_stops_the_open_before_anything_is_copied() {
    charter_core::unsteered!();
    let dir = tempfile::tempdir().expect("a directory");
    let root = firstrun::ensure_local_plane(
        &dir.path().join("config"),
        charter_core::firstrun::ForgeFrom::Named(charter_core::forge::Kind::GitHub),
    )
    .expect("a project");
    let repo = repo_with(dir.path(), "widget", "Cargo.toml");
    std::fs::write(root.join("opencode.json"), "[]")
        .expect("an opencode.json charter cannot extend");
    let settings = read(&root.join(".claude/settings.json"));

    let refused = firstrun::take_in_from(&root, &repo, &Choice::Fits).expect_err("refused");

    assert!(refused.contains("opencode.json"), "{refused}");
    assert!(
        !root.join("workspaces/widget").exists(),
        "nothing was copied"
    );
    assert!(!root.join("personas/rust-engineer").exists());
    assert_eq!(read(&root.join(".claude/settings.json")), settings);
}

#[cfg(unix)]
#[test]
fn a_template_that_fails_part_way_is_taken_back_whole() {
    charter_core::unsteered!();
    let dir = tempfile::tempdir().expect("a directory");
    let root = firstrun::ensure_local_plane(
        &dir.path().join("config"),
        charter_core::firstrun::ForgeFrom::Named(charter_core::forge::Kind::GitHub),
    )
    .expect("a project");
    let repo = repo_with(dir.path(), "widget", "Cargo.toml");
    // A workspace.md that reads, and that charter will not write: a link out of the project.
    // So the last step, the workspace's starter, fails after the personas, their sub-agents
    // and the ask rules are all written, and what the template wrote into both harness files
    // has to be put back byte for byte.
    let outside = dir.path().join("elsewhere.md");
    std::fs::write(
        &outside,
        "# widget\n\n## Context & decisions\n\n_Nothing yet._\n",
    )
    .expect("a file");
    std::fs::create_dir_all(root.join("workspaces/widget")).expect("the workspace");
    std::os::unix::fs::symlink(&outside, root.join("workspaces/widget/workspace.md"))
        .expect("a link");
    let settings = read(&root.join(".claude/settings.json"));
    let opencode = read(&root.join("opencode.json"));

    let refused = firstrun::take_in_from(&root, &repo, &Choice::Fits).expect_err("refused");

    assert!(refused.contains("workspace.md"), "{refused}");
    assert!(!root.join("personas/rust-engineer").exists(), "{refused}");
    assert!(!root.join("personas/rust-reviewer").exists());
    assert!(!root.join(".claude/agents/rust-engineer.md").exists());
    assert_eq!(read(&root.join(".claude/settings.json")), settings);
    assert_eq!(read(&root.join("opencode.json")), opencode);
}

#[cfg(unix)]
#[test]
fn a_workspace_md_charter_cannot_read_stops_the_template_and_is_never_removed() {
    charter_core::unsteered!();
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().expect("a directory");
    let root = firstrun::ensure_local_plane(
        &dir.path().join("config"),
        charter_core::firstrun::ForgeFrom::Named(charter_core::forge::Kind::GitHub),
    )
    .expect("a project");
    let repo = repo_with(dir.path(), "widget", "Cargo.toml");
    let file = root.join("workspaces/widget/workspace.md");
    std::fs::create_dir_all(file.parent().expect("its workspace")).expect("the workspace");
    std::fs::write(&file, "# widget\n\nOurs.\n").expect("the operator's");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).expect("unreadable");

    let refused = firstrun::take_in_from(&root, &repo, &Choice::Named("rust".to_owned()));

    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).expect("readable");
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(
        read(&file),
        "# widget\n\nOurs.\n",
        "never removed, never rewritten"
    );
    assert!(
        !root.join("personas/rust-engineer").exists(),
        "nothing was laid out"
    );
}

#[test]
fn taking_a_template_back_keeps_a_sub_agent_that_was_there_before() {
    charter_core::unsteered!();
    let dir = tempfile::tempdir().expect("a directory");
    let root = firstrun::ensure_local_plane(
        &dir.path().join("config"),
        charter_core::firstrun::ForgeFrom::Named(charter_core::forge::Kind::GitHub),
    )
    .expect("a project");
    let repo = repo_with(dir.path(), "widget", "Cargo.toml");
    // A sub-agent charter generated for an earlier `rust-engineer`, whose persona is gone.
    let agent = root.join(".claude/agents/rust-engineer.md");
    std::fs::create_dir_all(agent.parent().expect("agents")).expect("agents");
    let before = format!(
        "---\nname: rust-engineer\n---\n<!-- {} -->\nAn earlier one.\n",
        charter_core::personaverbs::agents::MARKER
    );
    std::fs::write(&agent, &before).expect("an earlier agent");
    // The last step fails, so the template is taken back.
    std::fs::create_dir_all(root.join("workspaces/widget/workspace.md")).expect("in the way");

    firstrun::take_in_from(&root, &repo, &Choice::Fits).expect_err("refused");

    assert!(
        agent.is_file(),
        "a sub-agent the template did not make stays"
    );
}

#[test]
fn a_project_this_charter_cannot_write_gets_no_template() {
    charter_core::unsteered!();
    let (_dir, root) = new_project();
    let manifest = root.join("charter.toml");
    let newer = read(&manifest).replace("schema = 1", "schema = 99");
    std::fs::write(&manifest, &newer).expect("a project from a newer charter");
    let settings = read(&root.join(".claude/settings.json"));

    let refused = template::apply(&root, template::named("rust").expect("Rust"), None)
        .expect_err("read-only (FR-24)");

    assert!(refused.contains("read-only"), "{refused}");
    assert!(!root.join("personas/rust-engineer").exists());
    assert_eq!(read(&root.join(".claude/settings.json")), settings);
}
