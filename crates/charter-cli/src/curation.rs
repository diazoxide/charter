//! `charter persona curation list|add|remove` and `charter curation show`: the command line of
//! [`charter_core::curation`] (ADR 0061).
//!
//! Two nouns, because they answer two questions. A persona's curation actions are files that
//! persona owns, so declaring and removing one are persona verbs, beside `persona create` and
//! `persona remove`. What a subject is *offered* is charter's own actions and every persona's
//! together, which belongs to no persona, so it is its own noun: `charter curation show
//! workspace:<name>` names the subject the way the app's menu is opened on one.

use std::io::{IsTerminal, Read};

use charter_core::curation::{self, Kind, New, RunsIn, Source, Subject};
use charter_core::personaverbs::lint::Level;
use clap::Subcommand;

use crate::memory::Code;
use crate::voice;

/// `charter persona curation …`.
#[derive(Subcommand)]
pub enum PersonaCuration {
    /// List the curation actions personas declare (personas/<name>/curation/<id>.md), and
    /// every problem that keeps one out of the menus.
    List {
        /// Only this persona (default: all).
        persona: Option<String>,
    },
    /// Declare a curation action for a persona. The prompt template is read from standard
    /// input; it may use {subject.kind}, {subject.name}, {subject.path} and {plane.root}.
    Add {
        persona: String,
        /// The action's id: lowercase letters, digits, '.', '_' and '-'.
        id: String,
        /// What the menu shows.
        #[arg(long)]
        label: String,
        /// The subjects it is offered on: workspace, persona, plane (comma-separated).
        #[arg(long, value_delimiter = ',', required = true)]
        on: Vec<String>,
        /// Where the chat starts: subject or plane (default: a workspace's own directory, the
        /// plane root for the others).
        #[arg(long)]
        runs_in: Option<String>,
    },
    /// Remove one of a persona's curation actions.
    Remove { persona: String, id: String },
}

/// `charter curation …`.
#[derive(Subcommand)]
pub enum CurationCommand {
    /// Print what a subject is offered, in menu order: who runs each action, where, and the
    /// prompt as it will be typed (never sent).
    Show {
        /// workspace:<name>, persona:<name>, or plane.
        subject: String,
    },
}

pub fn persona(root: &std::path::Path, command: PersonaCuration) -> Result<Code, String> {
    match command {
        PersonaCuration::List { persona } => Ok(list(root, persona.as_deref())),
        PersonaCuration::Add {
            persona,
            id,
            label,
            on,
            runs_in,
        } => {
            let mut kinds = Vec::new();
            for word in on.iter().map(|w| w.trim()).filter(|w| !w.is_empty()) {
                match Kind::parse(word) {
                    Some(kind) => kinds.push(kind),
                    None => {
                        voice::err(&format!(
                            "--on '{word}' is not a kind of subject — workspace, persona, plane"
                        ));
                        return Ok(2);
                    }
                }
            }
            let runs_in = match runs_in.as_deref() {
                None => None,
                Some(word) => match RunsIn::parse(word) {
                    Some(r) => Some(r),
                    None => {
                        voice::err(&format!("--runs-in '{word}' is neither subject nor plane"));
                        return Ok(2);
                    }
                },
            };
            let stdin = std::io::stdin();
            if stdin.is_terminal() {
                voice::err(
                    "the prompt is read from standard input — pipe it in, or end it with a \
                     here-document",
                );
                return Ok(2);
            }
            let mut prompt = String::new();
            stdin
                .lock()
                .read_to_string(&mut prompt)
                .map_err(|e| format!("could not read the prompt from standard input: {e}"))?;
            match curation::add(
                root,
                &New {
                    persona: &persona,
                    id: &id,
                    label: &label,
                    on: &kinds,
                    runs_in,
                    prompt: &prompt,
                },
            ) {
                Ok(_) => {
                    voice::ok(&format!(
                        "Curation action {persona}/{id} → personas/{persona}/curation/{id}.md"
                    ));
                    Ok(0)
                }
                Err(refused) => {
                    voice::err(&refused);
                    Ok(1)
                }
            }
        }
        PersonaCuration::Remove { persona, id } => match curation::remove(root, &persona, &id) {
            Ok(_) => {
                voice::ok(&format!(
                    "Removed curation action {persona}/{id} (personas/{persona}/curation/{id}.md)"
                ));
                Ok(0)
            }
            Err(refused) => {
                voice::err(&refused);
                Ok(1)
            }
        },
    }
}

fn list(root: &std::path::Path, only: Option<&str>) -> Code {
    let personas = match only {
        Some(name) => {
            let known = charter_core::workspaces::Plane::open(root)
                .personas()
                .unwrap_or_default()
                .iter()
                .any(|p| p == name);
            if !known {
                voice::err(&format!("no persona '{name}'"));
                return 1;
            }
            vec![name.to_string()]
        }
        None => charter_core::personagrant::list_personas(root),
    };
    let mut any = false;
    for persona in personas {
        for parsed in curation::declared(root, &persona) {
            any = true;
            match &parsed.action {
                Some(action) => {
                    let on: Vec<&str> = action.on.iter().map(|k| k.as_str()).collect();
                    println!("{persona}/{}  {}", action.id, action.label);
                    println!(
                        "  on: {} · runs in: {}",
                        on.join(", "),
                        action.runs_in.map_or("default", RunsIn::as_str)
                    );
                }
                None => println!("{persona}/{}  (not offered)", parsed.id),
            }
            for issue in &parsed.issues {
                let line = format!(
                    "personas/{persona}/curation/{}.md: {}",
                    parsed.id, issue.message
                );
                match issue.level {
                    Level::Error => voice::err(&line),
                    Level::Warn => voice::warn(&line),
                }
            }
        }
    }
    if !any {
        voice::info(
            "No curation actions are declared. A persona declares one as \
             personas/<name>/curation/<id>.md, or with `charter persona curation add`.",
        );
    }
    0
}

pub fn run(root: &std::path::Path, command: CurationCommand) -> Result<Code, String> {
    match command {
        CurationCommand::Show { subject } => {
            let subject = match Subject::parse(&subject) {
                Ok(subject) => subject,
                Err(refused) => {
                    voice::err(&refused);
                    return Ok(2);
                }
            };
            let resolution = match curation::resolve(root, &subject) {
                Ok(resolution) => resolution,
                Err(refused) => {
                    voice::err(&refused);
                    return Ok(1);
                }
            };
            for (n, action) in resolution.actions.iter().enumerate() {
                if n > 0 {
                    println!();
                }
                let source = match &action.source {
                    Source::Charter => "charter".to_string(),
                    Source::Persona(p) => format!("persona {p}"),
                };
                println!("{}  {}", action.id, action.label);
                println!(
                    "  source: {source} · runs as: {} · in: {}",
                    action.runner.as_deref().unwrap_or("no persona"),
                    action.cwd.display()
                );
                for line in action.prompt.lines() {
                    println!("    {line}");
                }
            }
            if resolution.actions.is_empty() {
                voice::info("Nothing is offered on this subject.");
            }
            for warning in &resolution.warnings {
                voice::warn(warning);
            }
            Ok(0)
        }
    }
}
