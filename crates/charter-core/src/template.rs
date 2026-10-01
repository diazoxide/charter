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

/// What a template is for, which is what [`detect`] picks it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// One code stack: Rust, Go, …
    Stack,
    /// Several stacks in one repo. Its ask rules are its own and every stack's, so a repo that
    /// holds a Go module and a Python package is asked about both.
    Monorepo,
    /// Documentation with no code beside it.
    Docs,
}

/// One project template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// Its directory's name: `rust`, `docs`.
    pub id: String,
    pub kind: Kind,
    /// What the window calls it.
    pub title: String,
    /// One line on what it is for.
    pub summary: String,
    /// A published version's files never change: a change is the next version.
    pub version: u32,
    /// A repo is this template's when one of these files is at its top level.
    pub detect: Vec<String>,
    /// Commands every harness with command permissions asks the operator about before a chat
    /// runs them.
    pub ask: Vec<String>,
    /// Every file it lays out, by its path in the project, with its text.
    pub files: Vec<(String, &'static str)>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    kind: Kind,
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
        // Each directory's files, by its id, in the order `build.rs` sorted them.
        let mut dirs: Vec<(&str, Vec<(String, &'static str)>)> = Vec::new();
        for (key, text) in data::FILES {
            let Some((id, rel)) = key.split_once('/') else {
                continue;
            };
            match dirs.iter_mut().find(|(have, _)| *have == id) {
                Some((_, files)) => files.push((rel.to_owned(), *text)),
                None => dirs.push((id, vec![(rel.to_owned(), *text)])),
            }
        }
        let mut found: Vec<Template> = dirs
            .into_iter()
            .map(|(id, mut files)| {
                let at = files
                    .iter()
                    .position(|(rel, _)| rel == MANIFEST)
                    .unwrap_or_else(|| panic!("templates/{id} has no {MANIFEST}"));
                let (_, text) = files.remove(at);
                // Compiled in, and every one is read by this crate's tests: a template that
                // does not parse is a defect of the build, never something a user can cause.
                let manifest: Manifest = toml::from_str(text).unwrap_or_else(|why| {
                    panic!("templates/{id}/{MANIFEST} does not parse: {why}")
                });
                Template {
                    id: id.to_owned(),
                    kind: manifest.kind,
                    title: manifest.title,
                    summary: manifest.summary,
                    version: manifest.version,
                    detect: manifest.detect.files,
                    ask: manifest.guard.ask,
                    files,
                }
            })
            .collect();
        // A monorepo is asked about whatever any stack in it would be, derived here so the two
        // lists cannot drift apart.
        let stacks: Vec<String> = found
            .iter()
            .filter(|one| one.kind == Kind::Stack)
            .flat_map(|one| one.ask.iter().cloned())
            .collect();
        for one in found.iter_mut().filter(|one| one.kind == Kind::Monorepo) {
            for rule in &stacks {
                if !one.ask.contains(rule) {
                    one.ask.push(rule.clone());
                }
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

/// The template that fits the repo at `repo`, by the files at its top level, or `None` when
/// none does. It asks only whether each file is there, as a file and not a link to one:
/// nothing in the repo is read, and nothing outside it is asked about.
///
/// One code stack is that stack's template. Two of them, or a monorepo's own workspace file
/// (`pnpm-workspace.yaml`, `go.work`, …), are a monorepo. A docs site is docs only when there
/// is no code beside it: a Go module with an `mkdocs.yml` is a Go repo that has docs.
pub fn detect(repo: &std::path::Path) -> Option<&'static Template> {
    let has = |one: &Template| {
        one.detect.iter().any(|file| {
            std::fs::symlink_metadata(repo.join(file)).is_ok_and(|meta| meta.file_type().is_file())
        })
    };
    let of = |kind: Kind| all().iter().filter(move |one| one.kind == kind);
    let stacks: Vec<&Template> = of(Kind::Stack).filter(|one| has(one)).collect();
    if let Some(monorepo) = of(Kind::Monorepo).find(|one| has(one)) {
        return Some(monorepo);
    }
    match stacks.as_slice() {
        [one] => Some(one),
        [] => of(Kind::Docs).find(|one| has(one)),
        _ => of(Kind::Monorepo).next(),
    }
}

/// The file in a template that seeds a new workspace's `workspace.md`, not the project root's.
const WORKSPACE_STARTER: &str = "workspace.md";

/// What [`apply`] put in a project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    /// The files written, by their path in the project.
    pub written: Vec<String>,
    /// The ask rules added in at least one harness, as Claude Code spells them.
    pub asked: Vec<String>,
    /// The ask rules at least one harness already denies, and was left denying: a template's
    /// rule never makes a denied command one the operator can answer.
    pub denied: Vec<String>,
}

/// Whether `template` can be laid into the project at `root`, asked with nothing written: the
/// project is one this charter may write (FR-24), and every harness file it writes a rule into
/// can take it. The reason it cannot, in the words [`apply`] would refuse with.
pub fn check(root: &std::path::Path, template: &Template) -> Result<(), String> {
    // A project this charter may not write is read-only to it (FR-24), and a template is
    // nothing but writes.
    if let crate::compat::Compat::ReadOnly(why) = crate::compat::read(root) {
        return Err(format!(
            "This project is read-only to this charter: {why}. Nothing from the {} template was \
             written.",
            template.title
        ));
    }
    for pattern in &template.ask {
        let rule = crate::guardcmd::as_rule(pattern)?;
        refused(
            template,
            &rule,
            &crate::guardcmd::check(root, &rule, ASK, false),
        )?;
    }
    Ok(())
}

const ASK: crate::guardcmd::Bucket = crate::guardcmd::Bucket::Ask;

/// `Err` naming every harness whose file would not take `rule`.
fn refused(
    template: &Template,
    rule: &str,
    answers: &[(&str, crate::guardcmd::Answer)],
) -> Result<(), String> {
    let refused: Vec<String> = answers
        .iter()
        .filter_map(|(harness, answer)| match answer {
            crate::guardcmd::Answer::Malformed(why) | crate::guardcmd::Answer::Unwritable(why) => {
                Some(format!("{harness}: {why}"))
            }
            _ => None,
        })
        .collect();
    if refused.is_empty() {
        return Ok(());
    }
    Err(format!(
        "charter could not add the {} template's ask rule {rule}: {}. Nothing from the template \
         was written. Fix that file, or open the repo with no template.",
        template.title,
        refused.join("; ")
    ))
}

/// Lays `template` into the project at `root`: its personas, each with the `memory/` and
/// `refs/` every persona has and its sub-agent; the starter of `workspace`, when one is named
/// ([`seed_workspace`]); and its ask rules in every harness that holds command permissions
/// (Claude Code and opencode). Codex's command rules live in `CODEX_HOME` or a trusted project's `.codex/rules`, which charter does not write, so charter's own guard applies there.
///
/// **Additive, as `charter init` is.** A persona whose directory is already there is left whole,
/// so a template never writes into a role somebody already has; any other file that is there is
/// left as it is; an ask rule already in force stays as it was, and one a harness already
/// denies stays denied ([`Applied::denied`]). Applying a template twice is applying it once.
/// Nothing is written through a link.
///
/// **Whole, or not at all.** Every harness file is asked first ([`check`]). Then the personas
/// and their sub-agents are laid out, the ask rules written, and the workspace's starter given
/// last. When a step fails, everything this call wrote is taken back — the persona directories
/// it made, the sub-agents that were not there before it, and the harness files and
/// `workspace.md` byte for byte as they were — and the reason is the answer. A file charter
/// cannot read to keep stops it before anything is laid out, since it could not be put back.
/// Only when it all landed is every workspace layer rewritten to carry the rules.
pub fn apply(
    root: &std::path::Path,
    template: &Template,
    workspace: Option<&str>,
) -> Result<Applied, String> {
    check(root, template)?;
    apply_checked(root, template, workspace)
}

/// [`apply`] for a caller that has just had [`check`] answer, so the harness files are asked
/// once (the first run asks before it copies the repo).
pub(crate) fn apply_checked(
    root: &std::path::Path,
    template: &Template,
    workspace: Option<&str>,
) -> Result<Applied, String> {
    let mut kept = vec![
        root.join(crate::scaffold::settings::SETTINGS),
        root.join(crate::scaffold::settings::OPENCODE),
    ];
    if let Some(ws) = workspace {
        kept.push(root.join("workspaces").join(ws).join(WORKSPACE_STARTER));
    }
    // What each file holds now, so a failure can put it back. Only a file that is not there
    // is "absent": one charter cannot read is one it could not restore, so nothing is laid out.
    let mut before: Vec<(std::path::PathBuf, Option<Vec<u8>>)> = Vec::new();
    for path in kept {
        let was = match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                return Err(format!(
                    "charter cannot read {} ({e}), so it could not put it back if laying out the \
                     {} template failed. Nothing from the template was written.",
                    path.display(),
                    template.title
                ));
            }
        };
        before.push((path, was));
    }
    let mut made = Made::default();
    match lay(root, template, workspace, &mut made) {
        Ok(applied) => {
            if !applied.asked.is_empty() {
                crate::guardcmd::mirror(root);
            }
            Ok(applied)
        }
        Err(why) => {
            made.take_back(root);
            for (path, was) in before {
                let _ = match was {
                    Some(bytes) => {
                        crate::rewrite::replace(root, &path, &bytes, crate::rewrite::Mode::Kept)
                    }
                    None => std::fs::remove_file(&path).or_else(|e| {
                        if e.kind() == std::io::ErrorKind::NotFound {
                            Ok(())
                        } else {
                            Err(e)
                        }
                    }),
                };
            }
            Err(why)
        }
    }
}

/// What [`apply`] made that was not there before it ran, so a failure can take it back.
#[derive(Default)]
struct Made {
    /// Persona directories it created: everything in them is its own.
    personas: Vec<std::path::PathBuf>,
    /// Sub-agents it generated.
    agents: Vec<std::path::PathBuf>,
}

impl Made {
    fn take_back(&self, root: &std::path::Path) {
        for agent in &self.agents {
            let _ = std::fs::remove_file(agent);
        }
        for dir in &self.personas {
            // Only a real directory under the project: never through a link that appeared
            // since.
            let real = std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.is_dir())
                && crate::contain::no_link_on_the_way(root, dir).is_ok();
            if real {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }
}

/// [`apply`]'s steps, in order, recording in `made` what each one created.
fn lay(
    root: &std::path::Path,
    template: &Template,
    workspace: Option<&str>,
    made: &mut Made,
) -> Result<Applied, String> {
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
    for persona in &personas {
        let dir = root.join("personas").join(persona);
        crate::contain::no_link_on_the_way(root, &dir)
            .and_then(|()| std::fs::create_dir_all(&dir))
            .map_err(|why| format!("charter could not make {} ({why}).", dir.display()))?;
        made.personas.push(dir);
    }
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
        let rel = format!(".claude/agents/{persona}.md");
        // Only an agent this call brings into being is its to take back: one that was there
        // (an earlier persona's, refreshed now) stays.
        let new = std::fs::symlink_metadata(root.join(&rel)).is_err();
        let mut quiet = |_: crate::repocmd::Say| {};
        if crate::personaverbs::agents::write_agent(root, &state, persona, &mut quiet)
            == crate::personaverbs::agents::Outcome::Written
        {
            if new {
                made.agents.push(root.join(&rel));
            }
            applied.written.push(rel);
        }
    }
    for pattern in &template.ask {
        let rule = crate::guardcmd::as_rule(pattern)?;
        let (answers, _) = crate::guardcmd::apply(root, &rule, ASK, false);
        refused(template, &rule, &answers)?;
        let any = |wanted: fn(&crate::guardcmd::Answer) -> bool| {
            answers.iter().any(|(_, answer)| wanted(answer))
        };
        if any(|answer| matches!(answer, crate::guardcmd::Answer::Denied(_))) {
            applied.denied.push(rule.clone());
        }
        if any(|answer| matches!(answer, crate::guardcmd::Answer::Added(_))) {
            applied.asked.push(rule);
        }
    }
    if let Some(ws) = workspace
        && seed_workspace(root, ws, template)?
    {
        applied
            .written
            .push(format!("workspaces/{ws}/{WORKSPACE_STARTER}"));
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
