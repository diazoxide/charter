//! Project templates (FR-17, N18): a new project laid out for the stack its repo is in.
//!
//! **A template is data.** Each one is a directory under `crates/charter-core/templates/`, laid
//! out as the files it puts in a project, beside a `template.toml` that names it, says which
//! repos it is for and which commands every harness asks about first. `build.rs` compiles the
//! directory in, so the template a first run lays out is the one this install ships, and adding
//! a stack is adding a directory. Each template has a `version`, which goes up by one whenever
//! any of its files changes; a test holds every version to the files it was published with.
//!
//! The words are ADR 0072's: the operator sees a **project template**; the code's `root` is the
//! plane the project is.

use std::sync::OnceLock;

mod data {
    include!(concat!(env!("OUT_DIR"), "/template_files.rs"));
}

/// The file in each template's directory that describes it rather than being copied.
const MANIFEST: &str = "template.toml";

/// One project template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// Its directory's name: `rust`, `docs`.
    pub id: String,
    /// What the window calls it.
    pub title: String,
    /// One line on what it is for.
    pub summary: String,
    /// Up by one whenever any of its files changes.
    pub version: u32,
    /// A repo is this stack when one of these files is at its top level.
    pub detect: Vec<String>,
    /// Commands every harness asks the operator about before a chat runs them.
    pub ask: Vec<String>,
    /// Every file it lays out, by its path in the project, with its text.
    pub files: Vec<(String, &'static str)>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    title: String,
    summary: String,
    version: u32,
    detect: Detect,
    guard: Guard,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Detect {
    files: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Guard {
    ask: Vec<String>,
}

/// Every template this charter ships, by id.
pub fn all() -> &'static [Template] {
    static ALL: OnceLock<Vec<Template>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut found: Vec<Template> = Vec::new();
        for (key, text) in data::FILES {
            let Some((id, rel)) = key.split_once('/') else {
                continue;
            };
            let at = match found.iter().position(|one| one.id == id) {
                Some(at) => at,
                None => {
                    found.push(Template {
                        id: id.to_owned(),
                        title: String::new(),
                        summary: String::new(),
                        version: 0,
                        detect: Vec::new(),
                        ask: Vec::new(),
                        files: Vec::new(),
                    });
                    found.len() - 1
                }
            };
            let one = &mut found[at];
            if rel == MANIFEST {
                // Compiled in, and every one is read by this crate's tests: a template that
                // does not parse is a defect of the build, never something a user can cause.
                let manifest: Manifest = toml::from_str(text)
                    .unwrap_or_else(|why| panic!("templates/{key} does not parse: {why}"));
                one.title = manifest.title;
                one.summary = manifest.summary;
                one.version = manifest.version;
                one.detect = manifest.detect.files;
                one.ask = manifest.guard.ask;
            } else {
                one.files.push((rel.to_owned(), *text));
            }
        }
        found.sort_by(|a, b| a.id.cmp(&b.id));
        found
    })
}

/// The template called `id`, if this charter ships one.
pub fn named(id: &str) -> Option<&'static Template> {
    all().iter().find(|one| one.id == id)
}

/// The template for several stacks in one repo, and the one detected when two stacks are.
pub const MONOREPO: &str = "monorepo";

/// The template for a repo of documentation, detected only when no code stack is.
pub const DOCS: &str = "docs";

/// The template that fits the repo at `repo`, by the files at its top level, or `None` when
/// none does. It asks only whether each file is there: nothing in the repo is read.
///
/// One code stack is that stack's template. Two of them, or a workspace file (`MONOREPO`'s own
/// `detect`), are a monorepo. A docs site is `DOCS` only when there is no code beside it: a
/// Go module with an `mkdocs.yml` is a Go repo that has docs.
pub fn detect(repo: &std::path::Path) -> Option<&'static Template> {
    let has = |one: &Template| one.detect.iter().any(|file| repo.join(file).is_file());
    let stacks: Vec<&Template> = all()
        .iter()
        .filter(|one| one.id != MONOREPO && one.id != DOCS)
        .filter(|one| has(one))
        .collect();
    let workspace_file = named(MONOREPO).is_some_and(has);
    match stacks.as_slice() {
        _ if workspace_file => named(MONOREPO),
        [one] => Some(one),
        [] => named(DOCS).filter(|one| has(one)),
        _ => named(MONOREPO),
    }
}

/// The file in a template that seeds a new workspace's `workspace.md`, not the project root's.
const WORKSPACE_STARTER: &str = "workspace.md";

/// What [`apply`] put in a project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    /// The files written, by their path in the project.
    pub written: Vec<String>,
    /// The ask rules added, as Claude Code spells them.
    pub asked: Vec<String>,
}

/// Lays `template` into the project at `root`: its personas, each with the `memory/` and
/// `refs/` every persona has, and its ask rules in every harness that holds one.
///
/// **Additive, as `charter init` is.** A persona whose directory is already there is left whole,
/// so a template never writes into a role somebody already has; any other file that is there is
/// left as it is; an ask rule already in force stays as it was. Applying a template twice is
/// applying it once. Nothing is written through a link.
///
/// The rules go through `charter guard ask`'s writer, so they reach every harness or none, and
/// every workspace layer that exists is rewritten to carry them.
pub fn apply(root: &std::path::Path, template: &Template) -> Result<Applied, String> {
    let mut applied = Applied::default();
    // The personas this run makes: those whose directory is not there yet, anything at all
    // there (a dangling link included) counting as there.
    let mut personas: Vec<&str> = template
        .files
        .iter()
        .filter_map(|(rel, _)| persona_of(rel))
        .filter(|persona| std::fs::symlink_metadata(root.join("personas").join(persona)).is_err())
        .collect();
    personas.dedup();
    for (rel, text) in &template.files {
        let laid = match persona_of(rel) {
            Some(persona) => personas.contains(&persona),
            None => rel != WORKSPACE_STARTER,
        };
        if laid && create(root, rel, text)? {
            applied.written.push(rel.clone());
        }
    }
    let state = crate::personaverbs::state_dir(root);
    for persona in personas {
        for kept in ["memory/.gitkeep", "refs/.gitkeep"] {
            let rel = format!("personas/{persona}/{kept}");
            if create(root, &rel, "")? {
                applied.written.push(rel);
            }
        }
        // Its sub-agent, as `charter persona create` writes one, so a chat can hand work to it
        // from the start. A hand-written agent at the name is left alone, as it is there.
        let mut quiet = |_: crate::repocmd::Say| {};
        if crate::personaverbs::agents::write_agent(root, &state, persona, &mut quiet)
            == crate::personaverbs::agents::Outcome::Written
        {
            applied.written.push(format!(".claude/agents/{persona}.md"));
        }
    }
    for pattern in &template.ask {
        let rule = crate::guardcmd::as_rule(pattern)?;
        let (answers, blocked) =
            crate::guardcmd::apply(root, &rule, crate::guardcmd::Bucket::Ask, false);
        let refused: Vec<String> = answers
            .iter()
            .filter_map(|(harness, answer)| match answer {
                crate::guardcmd::Answer::Malformed(why)
                | crate::guardcmd::Answer::Unwritable(why) => Some(format!("{harness}: {why}")),
                _ => None,
            })
            .collect();
        if blocked || !refused.is_empty() {
            return Err(format!(
                "charter could not add the {} template's ask rule {rule}: {}. Fix that file, or \
                 start without a template.",
                template.title,
                refused.join("; ")
            ));
        }
        if answers
            .iter()
            .any(|(_, answer)| matches!(answer, crate::guardcmd::Answer::Added(_)))
        {
            applied.asked.push(rule);
        }
    }
    if !applied.asked.is_empty() {
        crate::guardcmd::mirror(root);
    }
    Ok(applied)
}

/// The persona a template file belongs to: `rust-reviewer` for `personas/rust-reviewer/…`.
fn persona_of(rel: &str) -> Option<&str> {
    let rest = rel.strip_prefix("personas/")?;
    rest.split_once('/').map(|(persona, _)| persona)
}

/// Writes `text` to `root/rel` when nothing is there, making its directories; `false` when
/// something already is. Refused when a link stands anywhere on the way.
fn create(root: &std::path::Path, rel: &str, text: &str) -> Result<bool, String> {
    let path = root.join(rel);
    let said = |why: std::io::Error| format!("charter could not write {} ({why}).", path.display());
    crate::contain::no_link_on_the_way(root, &path).map_err(said)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(said)?;
    }
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => std::io::Write::write_all(&mut file, text.as_bytes())
            .map(|()| true)
            .map_err(said),
        Err(why) if why.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(why) => Err(said(why)),
    }
}

/// Gives workspace `ws` of the project at `root` the template's starter `workspace.md`: each
/// `## ` section of it replaces the same section of the workspace's own file while that one
/// still says only what charter first wrote there. `false` when nothing was replaced.
pub fn seed_workspace(
    root: &std::path::Path,
    ws: &str,
    template: &Template,
) -> Result<bool, String> {
    let Some((_, starter)) = template
        .files
        .iter()
        .find(|(rel, _)| rel == WORKSPACE_STARTER)
    else {
        return Ok(false);
    };
    let headers: Vec<&str> = crate::mdsection::split_lines(starter)
        .into_iter()
        .filter_map(|line| line.strip_prefix("## "))
        .map(str::trim)
        .collect();
    let bodies: Vec<String> = headers
        .iter()
        .map(|header| crate::mdsection::section_body(starter, header))
        .collect();
    let sections: Vec<(&str, &str)> = headers
        .iter()
        .copied()
        .zip(bodies.iter().map(String::as_str))
        .collect();
    let workspace = crate::workspaces::Plane::open(root)
        .workspace(ws)
        .map_err(|why| format!("{ws}: {why}"))?;
    workspace.seed_sections(&sections).map_err(|why| {
        format!(
            "charter could not start {}'s workspace.md from the {} template ({why}).",
            ws, template.title
        )
    })
}
