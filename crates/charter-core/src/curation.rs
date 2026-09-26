//! Curation actions: a chat the operator opens on a workspace, a persona or the plane with a
//! prompt already typed into it, and never sent (ADR 0061).
//!
//! Not memory curation, which is [`crate::curate`]: that finds what a memory store could lose.
//! A curation action is a *chat*, and what it does is up to the operator reading its prompt
//! before pressing Enter. That rule has no opt-out, and nothing here sends anything: this module
//! only says which actions a subject is offered, who runs each one, where, and with what text.
//!
//! # Two sources, one list
//!
//! - **charter's own**, [`BUILTINS`]: shipped in the binary, never files, ids `charter/<id>`,
//!   always listed first and in a fixed order.
//! - **A persona's**, one file each at `personas/<persona>/curation/<id>.md`
//!   (`docs/plane-format.md`): a line-based frontmatter (`label`, `on`, `runs-in`) and a body
//!   that is the prompt template. The declaring persona runs it, and its id is `<persona>/<id>`.
//!
//! A persona's file cannot override or impersonate a built-in: one whose id or label is a
//! built-in's is an error. [`parse`] is the one place that decides what is wrong with a file,
//! and three readers ask it — [`resolve`] (which leaves a broken action out *and says so*),
//! [`lint`] (which `charter persona lint` reports) and [`add`] (which refuses to write one) —
//! so no two of them can disagree about whether an action is well-formed.
//!
//! # The template is data, not a program
//!
//! Four variables and nothing else: `{subject.kind}`, `{subject.name}`, `{subject.path}`,
//! `{plane.root}`. One plain pass: a substituted value is never scanned again, no shell sees
//! the text, and no environment variable, vault or secret is read. Any other `{word}` is an
//! error, so a typo is caught in `lint` rather than typed into a chat.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::personaverbs::lint::{Issue, Level};

/// The kind of thing a curation action is offered on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Workspace,
    Persona,
    Plane,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Workspace, Kind::Persona, Kind::Plane];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Workspace => "workspace",
            Kind::Persona => "persona",
            Kind::Plane => "plane",
        }
    }

    pub fn parse(word: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.as_str() == word)
    }
}

/// Where the chat starts, when an action says: `runs-in: subject | plane`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunsIn {
    /// The subject's own directory: the workspace's, `personas/<name>/`, or the plane root.
    Subject,
    Plane,
}

impl RunsIn {
    pub fn as_str(self) -> &'static str {
        match self {
            RunsIn::Subject => "subject",
            RunsIn::Plane => "plane",
        }
    }

    pub fn parse(word: &str) -> Option<RunsIn> {
        match word {
            "subject" => Some(RunsIn::Subject),
            "plane" => Some(RunsIn::Plane),
            _ => None,
        }
    }
}

/// Whose action it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// One of [`BUILTINS`].
    Charter,
    /// Declared in `personas/<name>/curation/`.
    Persona(String),
}

/// What an action is opened on: a workspace or a persona by name, or the plane itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    pub kind: Kind,
    /// The workspace's or persona's name; `None` for the plane.
    pub name: Option<String>,
}

impl Subject {
    /// `workspace:<name>`, `persona:<name>` or `plane` — how the command line names one.
    pub fn parse(spec: &str) -> Result<Subject, String> {
        let refuse =
            || format!("'{spec}' is not a subject — workspace:<name>, persona:<name> or plane");
        if spec == "plane" {
            return Ok(Subject {
                kind: Kind::Plane,
                name: None,
            });
        }
        let (kind, name) = spec.split_once(':').ok_or_else(refuse)?;
        let kind = Kind::parse(kind)
            .filter(|k| *k != Kind::Plane)
            .ok_or_else(refuse)?;
        if name.is_empty() {
            return Err(refuse());
        }
        Ok(Subject {
            kind,
            name: Some(name.to_string()),
        })
    }
}

/// A persona's action, read from its file with nothing wrong in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    pub persona: String,
    /// The file's stem. The full id is `<persona>/<id>`.
    pub id: String,
    pub label: String,
    pub on: Vec<Kind>,
    /// `None` when the file does not say, and the subject's kind decides ([`cwd`]).
    pub runs_in: Option<RunsIn>,
    /// The prompt template: the body under the frontmatter, stripped.
    pub template: String,
}

/// One file under `curation/`, as [`parse`] read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub id: String,
    /// The kinds it names, when it names any it can be read for; `None` when the file gives
    /// no `on:` at all, so which lists it was meant for is unknown.
    pub on: Option<Vec<Kind>>,
    /// The action, when nothing in the file is an error.
    pub action: Option<Declared>,
    /// Every finding, errors and warnings, in the order they were found.
    pub issues: Vec<Issue>,
}

impl Parsed {
    fn errors(&self) -> impl Iterator<Item = &str> {
        self.issues
            .iter()
            .filter(|i| i.level == Level::Error)
            .map(|i| i.message.as_str())
    }
}

/// The values the four template variables take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vars {
    pub kind: Kind,
    pub name: String,
    pub path: String,
    pub plane_root: String,
}

/// One action a subject is offered, ready for the app to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// `charter/<id>` or `<persona>/<id>`.
    pub id: String,
    pub label: String,
    pub source: Source,
    /// The persona the chat starts as, or `None` for no persona.
    pub runner: Option<String>,
    /// The directory the chat starts in.
    pub cwd: PathBuf,
    /// The rendered prompt, to be typed into the chat and never sent.
    pub prompt: String,
}

/// What a subject is offered, and every action left out of it for an error, named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub actions: Vec<Resolved>,
    pub warnings: Vec<String>,
}

/// One of charter's own actions.
pub struct Builtin {
    /// The short id; the full one is `charter/<id>`.
    pub id: &'static str,
    pub label: &'static str,
    /// The skill in charter's plugin that its prompt names.
    pub skill: &'static str,
    pub on: &'static [Kind],
    /// Always at the plane root, whatever the subject: Safe remove's subject is going away.
    pub at_plane_root: bool,
    /// The prompt template for a subject of this kind.
    pub template: fn(Kind) -> &'static str,
}

/// charter's own, in the order every list shows them.
pub const BUILTINS: [Builtin; 3] = [
    Builtin {
        id: "safe-remove",
        label: "Safe remove",
        skill: "safe-remove",
        on: &[Kind::Workspace, Kind::Persona],
        at_plane_root: true,
        template: safe_remove,
    },
    Builtin {
        id: "compact",
        label: "Compact & improve",
        skill: "compact",
        on: &[Kind::Workspace, Kind::Persona],
        at_plane_root: false,
        template: compact,
    },
    Builtin {
        id: "add-curation-action",
        label: "Add curation action",
        skill: "add-curation-action",
        on: &[Kind::Persona],
        at_plane_root: false,
        template: add_curation_action,
    },
];

fn safe_remove(kind: Kind) -> &'static str {
    match kind {
        Kind::Persona => {
            "Safely remove the persona `{subject.name}` ({subject.path}), using charter's \
`safe-remove` skill.

Audit it first: its charter, its memory and its curation actions. Promote every durable \
learning that should outlive it to shared memory, another persona's memory, or this plane's \
docs. Show me what you promoted and what you are leaving behind, then run \
`charter persona remove {subject.name}`. Its guards still apply: if another persona extends \
or uses it, stop and tell me. Never pass --force unless I say so."
        }
        _ => {
            "Safely remove the workspace `{subject.name}` ({subject.path}), using charter's \
`safe-remove` skill.

Audit it first: its workspace.md, its memory, its todos, and the work in its repos. Promote \
every durable learning to where it outlives the workspace: shared memory, a persona's memory, \
or this plane's docs. Show me what you promoted and what you are leaving behind, then run \
`charter workspace remove {subject.name}`. Its guards still apply: if it refuses, stop and \
tell me why. Never pass --force unless I say so."
        }
    }
}

fn compact(kind: Kind) -> &'static str {
    match kind {
        Kind::Persona => {
            "Compact and improve the persona `{subject.name}`, using charter's `compact` skill.

Run `charter persona optimize {subject.name}` and `charter persona dedupe {subject.name}`, and \
read what they report. Apply the safe operations only once I agree, and forget a memory only \
with my yes. Then fold the lessons that have become durable into its persona.md, in \
{subject.path}, and run `charter persona sync-agents` if you changed it."
        }
        _ => {
            "Compact and improve the workspace `{subject.name}`, using charter's `compact` skill.

Run `charter workspace optimize {subject.name}` and read its report. Apply the safe \
operations only once I agree, and forget a memory only with my yes. Then fold the lessons \
that have become durable into its workspace.md, in {subject.path}, so the next chat starts \
from them rather than from the journal."
        }
    }
}

fn add_curation_action(_kind: Kind) -> &'static str {
    "Add a curation action to the persona `{subject.name}`, using charter's \
`add-curation-action` skill.

Ask me what it should do, which subjects it is offered on (workspace, persona, plane) and \
where it should run. Then write it with `charter persona curation add {subject.name} <id> \
--label \"<label>\" --on <kinds>`, the prompt on standard input, and show me what \
`charter persona curation list {subject.name}` says."
}

/// The frontmatter keys an action file may give.
const KEYS: [&str; 3] = ["label", "on", "runs-in"];

/// The variables a template may use, in the order a refusal lists them.
const VARIABLES: [&str; 4] = ["subject.kind", "subject.name", "subject.path", "plane.root"];

/// What is wrong with one action file's text, and the action when nothing is.
///
/// `persona` and `id` are the file's place (`personas/<persona>/curation/<id>.md`); the text is
/// what it holds. Nothing here reads the disk.
pub fn parse(persona: &str, id: &str, text: &str) -> Parsed {
    let mut issues = Vec::new();
    let error = |issues: &mut Vec<Issue>, message: String| {
        issues.push(Issue {
            level: Level::Error,
            message,
        })
    };
    if !crate::personas::valid_name(id) {
        error(
            &mut issues,
            format!("'{id}' is not an action id — lowercase letters, digits, '.', '_' and '-'"),
        );
    }
    let pairs = crate::personas::frontmatter(text);
    let mut seen = BTreeSet::new();
    for (key, _) in &pairs {
        if !seen.insert(key.as_str()) {
            error(&mut issues, format!("{key}: is given twice"));
        } else if !KEYS.contains(&key.as_str()) {
            issues.push(Issue {
                level: Level::Warn,
                message: format!("unknown key '{key}' — charter reads label, on and runs-in"),
            });
        }
    }
    let get = |key: &str| {
        pairs
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };

    let label = get("label").unwrap_or_default().to_string();
    if label.is_empty() {
        error(
            &mut issues,
            "no label: — the menu has nothing to show".into(),
        );
    }

    // `None` when the file gives no `on:`; otherwise the kinds it names that charter knows,
    // each once. A word that is not a kind is its own error, so "names nothing" is said only
    // when the list was empty to begin with.
    const NO_ON: &str = "no on: — name the subjects it is offered on: workspace, persona, plane";
    let words = get("on").map(|raw| crate::personagrant::csv_list(Some(raw)));
    if words.as_ref().is_none_or(Vec::is_empty) {
        error(&mut issues, NO_ON.into());
    }
    let on = words.map(|words| {
        let mut kinds = Vec::new();
        for word in words {
            match Kind::parse(&word) {
                Some(kind) if !kinds.contains(&kind) => kinds.push(kind),
                Some(_) => {}
                None => error(
                    &mut issues,
                    format!("on: '{word}' is not a kind of subject — workspace, persona, plane"),
                ),
            }
        }
        kinds
    });

    let runs_in = match get("runs-in") {
        None => None,
        Some(word) => match RunsIn::parse(word) {
            Some(r) => Some(r),
            None => {
                error(
                    &mut issues,
                    format!("runs-in: '{word}' is neither subject nor plane"),
                );
                None
            }
        },
    };

    if let Some(builtin) = BUILTINS.iter().find(|b| b.id == id) {
        error(
            &mut issues,
            format!(
                "the id '{id}' is charter's own (charter/{}), which a persona cannot override — \
                 rename the file",
                builtin.id
            ),
        );
    }
    if let Some(builtin) = BUILTINS
        .iter()
        .find(|b| !label.is_empty() && b.label.to_lowercase() == label.to_lowercase())
    {
        error(
            &mut issues,
            format!(
                "the label '{label}' is charter's own (charter/{}), which a persona cannot \
                 take — choose another",
                builtin.id
            ),
        );
    }

    let template = crate::personas::charter_body(text);
    if template.is_empty() {
        error(
            &mut issues,
            "the prompt is empty — write it below the frontmatter".into(),
        );
    }
    for unknown in template_problems(&template) {
        error(
            &mut issues,
            format!(
                "the prompt uses {{{unknown}}}, which is not a variable — only {{subject.kind}}, \
                 {{subject.name}}, {{subject.path}} and {{plane.root}} are"
            ),
        );
    }

    let failed = issues.iter().any(|i| i.level == Level::Error);
    let action = (!failed).then(|| Declared {
        persona: persona.to_string(),
        id: id.to_string(),
        label: label.clone(),
        on: on.clone().unwrap_or_default(),
        runs_in,
        template: template.clone(),
    });
    Parsed {
        id: id.to_string(),
        on,
        action,
        issues,
    }
}

/// A `{word}` in a template: braces around one or more letters, digits, `_`, `.` or `-`.
/// Braces around anything else are text. Each is yielded as `(start, end, word)`, `end` past
/// the closing brace.
fn variables(template: &str) -> impl Iterator<Item = (usize, usize, &str)> {
    let bytes = template.as_bytes();
    let mut at = 0;
    std::iter::from_fn(move || {
        while at < bytes.len() {
            let open = at + template[at..].find('{')?;
            let word_len = template[open + 1..]
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
                .count();
            let close = open + 1 + word_len;
            if word_len > 0 && bytes.get(close) == Some(&b'}') {
                at = close + 1;
                return Some((open, close + 1, &template[open + 1..close]));
            }
            at = open + 1;
        }
        None
    })
}

/// Every `{word}` in `template` that is not one of the four variables, each once, in order.
pub fn template_problems(template: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (_, _, word) in variables(template) {
        if !VARIABLES.contains(&word) && !out.iter().any(|w| w == word) {
            out.push(word.to_string());
        }
    }
    out
}

/// `template` with the four variables substituted in one pass, or the unknown ones.
pub fn render(template: &str, vars: &Vars) -> Result<String, Vec<String>> {
    let unknown = template_problems(template);
    if !unknown.is_empty() {
        return Err(unknown);
    }
    let mut out = String::with_capacity(template.len());
    let mut from = 0;
    for (start, end, word) in variables(template) {
        out.push_str(&template[from..start]);
        out.push_str(match word {
            "subject.kind" => vars.kind.as_str(),
            "subject.name" => &vars.name,
            "subject.path" => &vars.path,
            _ => &vars.plane_root,
        });
        from = end;
    }
    out.push_str(&template[from..]);
    Ok(out)
}

/// `personas/<persona>/curation/`.
fn dir_of(root: &Path, persona: &str) -> PathBuf {
    root.join("personas").join(persona).join("curation")
}

/// Every action file a persona declares, read and judged, sorted by id. A persona with no
/// `curation/`, or a name that is not a persona's, declares none.
pub fn declared(root: &Path, persona: &str) -> Vec<Parsed> {
    if !crate::personas::valid_name(persona) {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(dir_of(root, persona)) else {
        return Vec::new();
    };
    let mut files: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let id = name.strip_suffix(".md")?.to_string();
            if name.starts_with('.') {
                return None;
            }
            let path = entry.path();
            // A directory named `x.md` is not an action; a link is judged by what it reaches,
            // and one that reaches nothing is reported as unreadable below.
            if std::fs::metadata(&path).is_ok_and(|m| m.is_dir()) {
                return None;
            }
            Some((id, path))
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|(id, path)| {
            let text = crate::memstore::readable_file(root, &path)
                .then(|| crate::memstore::read_text(&path))
                .flatten();
            match text {
                Some(text) => parse(persona, &id, &text),
                None => Parsed {
                    id,
                    on: None,
                    action: None,
                    issues: vec![Issue {
                        level: Level::Error,
                        message: "charter will not read it (outside the plane, not a regular \
                                  file, too large, or not UTF-8)"
                            .into(),
                    }],
                },
            }
        })
        .collect()
}

/// `charter persona lint`'s findings for a persona's curation actions, each under its file.
pub fn lint(root: &Path, persona: &str) -> Vec<Issue> {
    declared(root, persona)
        .into_iter()
        .flat_map(|parsed| {
            let id = parsed.id;
            parsed.issues.into_iter().map(move |issue| Issue {
                level: issue.level,
                message: format!("curation/{id}.md: {}", issue.message),
            })
        })
        .collect()
}

/// The directory an action on `kind` starts in when it does not say: a workspace's own, and
/// the plane root for a persona or the plane.
fn cwd(root: &Path, kind: Kind, subject_dir: &Path, runs_in: Option<RunsIn>) -> PathBuf {
    let runs_in = runs_in.unwrap_or(match kind {
        Kind::Workspace => RunsIn::Subject,
        Kind::Persona | Kind::Plane => RunsIn::Plane,
    });
    match runs_in {
        RunsIn::Subject => subject_dir.to_path_buf(),
        RunsIn::Plane => root.to_path_buf(),
    }
}

/// Every action `subject` is offered, in the order a menu shows them: charter's own first,
/// then each persona's, by persona and then by id. An action with an error is left out, and
/// named in [`Resolution::warnings`] whenever it was, or may have been, meant for this kind.
///
/// A subject that is not there is refused, in the words the plane's own lookups use.
pub fn resolve(root: &Path, subject: &Subject) -> Result<Resolution, String> {
    let plane = crate::workspaces::Plane::open(root);
    let (name, dir) = match (subject.kind, subject.name.as_deref()) {
        (Kind::Workspace, Some(name)) => {
            let known = crate::contain::workspace_name_ok(name)
                && plane
                    .workspaces()
                    .unwrap_or_default()
                    .iter()
                    .any(|w| w == name);
            if !known {
                return Err(format!("no workspace '{name}'"));
            }
            (name.to_string(), root.join("workspaces").join(name))
        }
        (Kind::Persona, Some(name)) => {
            let known = crate::personas::valid_name(name)
                && plane
                    .personas()
                    .unwrap_or_default()
                    .iter()
                    .any(|p| p == name);
            if !known {
                return Err(format!("no persona '{name}'"));
            }
            (name.to_string(), root.join("personas").join(name))
        }
        (Kind::Plane, None) => (
            root.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.display().to_string()),
            root.to_path_buf(),
        ),
        (kind, _) => {
            return Err(format!(
                "a {} subject {} a name",
                kind.as_str(),
                if kind == Kind::Plane {
                    "takes no"
                } else {
                    "needs"
                }
            ));
        }
    };
    let vars = Vars {
        kind: subject.kind,
        name: name.clone(),
        path: dir.display().to_string(),
        plane_root: root.display().to_string(),
    };
    let render = |template: &str| {
        render(template, &vars)
            .map_err(|unknown| format!("the prompt uses unknown variables: {}", unknown.join(", ")))
    };

    let mut actions = Vec::new();
    // Who runs charter's own: the persona being curated curates itself; anything else is run
    // by the persona this plane puts first, or by no persona. charter has no persona of its
    // own to name here (ADR 0061).
    let builtin_runner = match subject.kind {
        Kind::Persona => Some(name.clone()),
        _ => crate::active::plane_default_persona(root),
    };
    for builtin in BUILTINS.iter().filter(|b| b.on.contains(&subject.kind)) {
        let cwd = if builtin.at_plane_root {
            root.to_path_buf()
        } else {
            cwd(root, subject.kind, &dir, None)
        };
        actions.push(Resolved {
            id: format!("charter/{}", builtin.id),
            label: builtin.label.to_string(),
            source: Source::Charter,
            runner: builtin_runner.clone(),
            cwd,
            prompt: render((builtin.template)(subject.kind))?,
        });
    }

    let mut warnings = Vec::new();
    for persona in plane.personas().unwrap_or_default() {
        for parsed in declared(root, &persona) {
            match &parsed.action {
                Some(action) if action.on.contains(&subject.kind) => {
                    actions.push(Resolved {
                        id: format!("{persona}/{}", action.id),
                        label: action.label.clone(),
                        source: Source::Persona(persona.clone()),
                        runner: Some(persona.clone()),
                        cwd: cwd(root, subject.kind, &dir, action.runs_in),
                        prompt: render(&action.template)?,
                    });
                }
                Some(_) => {}
                None => {
                    let meant_here = parsed
                        .on
                        .as_ref()
                        .is_none_or(|on| on.contains(&subject.kind));
                    if meant_here {
                        warnings.push(format!(
                            "personas/{persona}/curation/{}.md is not offered: {}. \
                             `charter persona lint {persona}` lists every problem",
                            parsed.id,
                            parsed.errors().collect::<Vec<_>>().join("; ")
                        ));
                    }
                }
            }
        }
    }
    Ok(Resolution { actions, warnings })
}

/// A new action for [`add`].
pub struct New<'a> {
    pub persona: &'a str,
    pub id: &'a str,
    pub label: &'a str,
    pub on: &'a [Kind],
    pub runs_in: Option<RunsIn>,
    pub prompt: &'a str,
}

/// The persona's own directory, when `persona` is one this plane has in the directory layout.
fn persona_dir(root: &Path, persona: &str) -> Result<PathBuf, String> {
    let plane = crate::workspaces::Plane::open(root);
    let known = crate::personas::valid_name(persona)
        && plane
            .personas()
            .unwrap_or_default()
            .iter()
            .any(|p| p == persona);
    if !known {
        return Err(format!("no persona '{persona}'"));
    }
    let definition = crate::personas::def_path(root, persona);
    if definition.file_name().is_none_or(|f| f != "persona.md") {
        return Err(format!(
            "persona '{persona}' is the legacy flat file personas/{persona}.md, which has no \
             directory to hold curation actions"
        ));
    }
    Ok(root.join("personas").join(persona))
}

/// `charter persona curation add`: write `personas/<persona>/curation/<id>.md`, only when it
/// would read back with no error, and never over a file that is there. The path written.
pub fn add(root: &Path, new: &New) -> Result<PathBuf, String> {
    persona_dir(root, new.persona)?;
    if new.label.contains(['\n', '\r']) {
        return Err("a label is one line".into());
    }
    let on: Vec<&str> = new.on.iter().map(|k| k.as_str()).collect();
    let mut text = format!("---\nlabel: {}\non: {}\n", new.label.trim(), on.join(", "));
    if let Some(runs_in) = new.runs_in {
        text.push_str(&format!("runs-in: {}\n", runs_in.as_str()));
    }
    text.push_str(&format!("---\n\n{}\n", new.prompt.trim()));
    let parsed = parse(new.persona, new.id, &text);
    let errors: Vec<&str> = parsed.errors().collect();
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    let dir = dir_of(root, new.persona);
    let path = dir.join(format!("{}.md", new.id));
    let shown = format!("personas/{}/curation/{}.md", new.persona, new.id);
    if std::fs::symlink_metadata(&path).is_ok() {
        return Err(format!(
            "{shown} already exists — remove it first: charter persona curation remove {} {}",
            new.persona, new.id
        ));
    }
    crate::contain::writable(root, &dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {shown}: {e}"))?;
    crate::contain::writable(root, &path).map_err(|e| e.to_string())?;
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .map_err(|e| format!("could not write {shown}: {e}"))?;
    Ok(path)
}

/// `charter persona curation remove`: delete one action file. The path removed.
pub fn remove(root: &Path, persona: &str, id: &str) -> Result<PathBuf, String> {
    persona_dir(root, persona)?;
    if !crate::personas::valid_name(id) {
        return Err(format!(
            "'{id}' is not an action id — lowercase letters, digits, '.', '_' and '-'"
        ));
    }
    let path = dir_of(root, persona).join(format!("{id}.md"));
    if std::fs::symlink_metadata(&path).is_err() {
        return Err(format!("{persona} has no curation action '{id}'"));
    }
    crate::contain::writable(root, &path).map_err(|e| e.to_string())?;
    std::fs::remove_file(&path)
        .map_err(|e| format!("could not remove personas/{persona}/curation/{id}.md: {e}"))?;
    Ok(path)
}

#[cfg(test)]
#[path = "curation_tests.rs"]
mod tests;
