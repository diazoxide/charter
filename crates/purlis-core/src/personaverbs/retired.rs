//! What is left of the persona sub-agent (#1451, spec #1434 decision 2).
//!
//! A persona is a role a chat runs as for its whole life, and never a harness sub-agent. purlis
//! used to generate one Claude Code sub-agent per persona, `.claude/agents/<name>.md`, with
//! `purlis persona sync-agents`. It no longer writes or reads those files. What this module
//! keeps is what the retirement itself needs:
//!
//! - **the word**: `persona sync-agents` is still taken, and answers [`SYNC_AGENTS`];
//! - **the refusal** a sub-agent call named for a persona gets ([`subagent_refusal`]);
//! - **authorship**: which files under `.claude/agents/` purlis wrote ([`authorship`]), so the
//!   `persona-agents` fix ([`crate::doctor::fix::persona_agents`]) removes those and no other;
//! - **the keys** that only fed the generated file, and what became of each ([`KEYS`]).
//!
//! # Authorship is the marker, where the generator wrote it
//!
//! The generator left one proof on a file: the marker comment, as the first line under the
//! frontmatter, followed by the sentence that names the persona. It left no hash, and what it
//! rendered depended on the machine (a vault registry, an MCP approval), so a file cannot be
//! compared with a fresh rendering either. A file is purlis's when it has that shape, under
//! either spelling of the marker ([`crate::names::SYNC_AGENTS_MARKER`]). A file that carries the
//! marker anywhere else was edited by hand or only quotes it, and is left alone, as a file
//! without the marker is.
//!
//! What this cannot tell apart is a generated file whose charter text was edited by hand and
//! one that is merely stale. The marker line says "edit the persona, not this file", and every
//! `sync-agents` run overwrote such a file, so an edit under the marker was never kept.

use std::path::{Path, PathBuf};

/// What `purlis persona sync-agents` answers now: one sentence, and what replaced it.
pub const SYNC_AGENTS: &str = "`purlis persona sync-agents` is retired: a persona runs as its \
     own chat and purlis generates no helper for it. Give a persona work with `purlis \
     dispatch --to <persona>`, and remove the helper files purlis wrote with `purlis doctor \
     --fix persona-agents`.";

/// What a sub-agent call named for the persona `name` is refused with.
///
/// It names the route, `purlis dispatch --to <persona>`, and says a helper with no persona's
/// name still runs, so the chat does not conclude that sub-agents are gone. For a `draft`
/// persona the route is refused too (a draft runs no chat), so it says that instead.
pub fn subagent_refusal(name: &str, draft: bool) -> String {
    let name = crate::shown::short(name);
    if draft {
        return format!(
            "`{name}` is a persona, and a persona runs as its own chat, never as a helper of \
             this one. It is also still a draft, and a draft persona runs no chat: its \
             definition has to be finished and its `draft: true` line dropped before work can \
             be dispatched to it. Do the work in this chat, or dispatch to another persona. A \
             helper that is not named for a persona still runs, as this chat's persona."
        );
    }
    format!(
        "`{name}` is a persona, and a persona runs as its own chat, never as a helper of \
         this one: a helper works with this chat's vault and hosts, not `{name}`'s. \
         Dispatch to it instead: `purlis dispatch --to {name}`. A helper that is not named \
         for a persona still runs, as this chat's persona."
    )
}

/// The names a harness gives its own helpers. A persona called one of them takes the name
/// away from the helper: a call to it is refused with "is a persona" ([`subagent_refusal`]),
/// which `persona lint` warns about.
pub const HELPERS: [&str; 6] = [
    "general-purpose",
    "Explore",
    "Plan",
    "claude",
    "fork",
    "statusline-setup",
];

/// The keys of a persona's definition that only fed the generated sub-agent and that nothing
/// reads now. `persona lint` warns with [`key_notice`] and the `persona-agents` fix reports
/// it; neither removes the line, because the work that brings a key back reads it from where
/// the operator wrote it.
///
/// `model`, `color`, `description` and `agent-description` fed the file too and are not here:
/// each has a job now ([`crate::personaprofile`], [`crate::personamark`], [`description`]).
/// So has `disallowed-tools` ([`DENIED_TOOLS`]), and so has `dispatch-isolation` since #1453:
/// it is the persona's default place to work when a dispatch names none
/// ([`crate::dispatchplace::isolates`]). `skills` is here for what it did in a chat;
/// `persona stats` still reads it as the persona's declared skills.
pub const KEYS: [&str; 3] = ["agent-tools", "skills", "memory"];

/// The tools of a sub-agent's allow-list that write a file.
const WRITERS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// The sentence for a retired key and the `value` its line holds: `` `<key>:` is no longer
/// read: … ``, or `None` for a key that is not one of [`KEYS`]. It says **what widened**
/// (D-1451-19): a line that used to restrict a persona must not read as if it still did.
pub fn key_notice(key: &str, value: &str) -> Option<String> {
    let why = match key {
        "agent-tools" => {
            let listed: Vec<&str> = value
                .split(',')
                .map(crate::memstore::py_strip)
                .filter(|tool| !tool.is_empty())
                .collect();
            let widened = if listed.iter().any(|tool| WRITERS.contains(tool)) {
                "As a sub-agent this persona had only the tools listed; as a chat it has \
                 every tool of its harness."
            } else {
                "As a sub-agent this persona could not edit files; as a chat it can, and it \
                 has every other tool of its harness too."
            };
            format!(
                "it was the tool list of the generated sub-agent. {widened} Tool rules that \
                 allow one persona's chats only some tools are not in this version yet \
                 (#1460); `disallowed-tools:` denies a chat the ones it names"
            )
        }
        "skills" => "it preloaded those skills into the generated sub-agent, and a persona \
                     chat loads a skill when it uses it. Preloading a persona's skills into \
                     its chats is not in this version yet (#1460)"
            .to_owned(),
        "memory" => "it chose the harness's own memory store for the generated sub-agent. A \
                     persona chat has the persona's memory in `personas/<name>/memory/`, and \
                     choosing a harness store for it is not in this version yet (#1460)"
            .to_owned(),
        _ => return None,
    };
    Some(format!("`{key}:` is no longer read: {why}"))
}

/// What is said of a persona's `disallowed-tools:`: where it holds, and where the persona's
/// chat is refused instead ([`super::chatstart::unenforced`], D-1451-18).
pub const DENIED_TOOLS: &str = "`disallowed-tools:` is honoured on Claude Code: a chat as this \
     persona is started with those tools denied. On Codex and opencode purlis cannot deny \
     them, so a chat as this persona is refused there, and so is a dispatch to it";

/// What is said of a `model:` line that nothing reads, or `None` for one that names the
/// persona's profile ([`crate::personaprofile`]: `model:` is read where the chain has no
/// `profile:` line and the project offers a profile of exactly that name).
///
/// `chain_names_profile` is [`crate::personaprofile::chain_names_one`], and `offered` the
/// names of the profiles the project offers on this machine. It says what the chat runs on
/// instead, because that is a change of model and of cost (D-1451-20).
pub fn model_notice(model: &str, chain_names_profile: bool, offered: &[String]) -> Option<String> {
    let said = crate::personas::one_line(model);
    if chain_names_profile {
        return Some(format!(
            "`model: {said}` is no longer read: this persona's profile is named with \
             `profile:`. Delete the line"
        ));
    }
    if model != crate::personaprofile::NONE && offered.iter().any(|name| name == model) {
        return None;
    }
    Some(format!(
        "`model: {said}` is no longer read: it named a model for the generated sub-agent, and \
         this project offers no profile called that on this machine. A chat as this persona \
         runs on the asking chat's profile, with that profile's model and its cost, not on \
         `{said}`. Name the profile this persona's chats start on with `profile:`, or delete \
         the line"
    ))
}

/// What is said of a persona's `mcp.json` that declares `count` servers: where a chat as the
/// persona is started with them ([`super::chatstart`], D-1451-17).
pub fn servers_notice(count: usize) -> String {
    format!(
        "it declares {count} MCP server(s). A chat as this persona is started with them on \
         Claude Code; on Codex and opencode they are not started"
    )
}

/// The words of the old route that `charter`, a persona's charter text, still teaches: a chat
/// that follows them is refused. `persona lint` names them; nothing rewrites a charter.
pub fn old_route_in(charter: &str) -> Vec<&'static str> {
    ["subagent_type", "sync-agents"]
        .into_iter()
        .filter(|word| charter.contains(word))
        .collect()
}

/// The persona's one-line description: its `agent-description`, else its `description`, with
/// its `extends:` chain applied (`meta` is the merged frontmatter). It was the generated
/// sub-agent's `description:`; it is now what the persona says of itself in a line, quoted to
/// a chat that runs as it
/// ([`crate::briefing`]) and shown by `persona show`.
pub fn description(meta: &std::collections::BTreeMap<String, String>) -> Option<String> {
    ["agent-description", "description"]
        .into_iter()
        .filter_map(|key| meta.get(key))
        .map(|said| crate::personas::one_line(said))
        .find(|said| !said.is_empty())
}

/// The one-line description of the persona `name` ([`description`]), its `extends:` chain
/// applied, or `None` for one that declares none or does not load. What the new-chat picker
/// shows beside each persona (#1460).
pub fn description_of(root: &Path, name: &str) -> Option<String> {
    super::resolve(root, name).and_then(|def| description(&def.meta))
}

/// Where the generated sub-agents lived, under the tree.
pub fn agents_dir(root: &Path) -> PathBuf {
    root.join(".claude").join("agents")
}

/// Whether `text` carries the generator's marker anywhere, under either spelling. What
/// `rename-plane` rewrites; not what makes a file purlis's to remove ([`authorship`]).
pub fn carries_marker(text: &str) -> bool {
    crate::names::SYNC_AGENTS_MARKER
        .spellings()
        .any(|marker| text.contains(marker))
}

/// Who wrote a file under `.claude/agents/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorship {
    /// purlis generated it: the marker is where the generator wrote it, with the sentence
    /// that names the persona the file is called after.
    Generated,
    /// It carries the marker, but not in the shape purlis writes: edited by hand, renamed, or
    /// a file that only quotes the marker.
    Edited,
    /// No marker: somebody's own sub-agent.
    HandWritten,
}

/// Who wrote the agent file called `<stem>.md` that holds `text` (see the module header).
pub fn authorship(stem: &str, text: &str) -> Authorship {
    if !carries_marker(text) {
        return Authorship::HandWritten;
    }
    let generated = body_under_frontmatter(text).is_some_and(|body| {
        let mut lines = body.lines();
        let marker = lines.next().unwrap_or_default();
        let marked = crate::names::SYNC_AGENTS_MARKER
            .spellings()
            .any(|spelling| {
                marker
                    .strip_prefix("<!-- ")
                    .and_then(|rest| rest.strip_prefix(spelling))
                    .is_some_and(|rest| {
                        rest.starts_with(" from ")
                            && rest.ends_with(" — edit the persona, not this file. -->")
                    })
            });
        marked
            && lines.next() == Some("")
            && lines.next().is_some_and(|line| {
                line.starts_with(&format!("This sub-agent acts as the **{stem}** persona — "))
            })
    });
    if generated {
        Authorship::Generated
    } else {
        Authorship::Edited
    }
}

/// What follows the frontmatter the generator opened every file with, or `None` when `text`
/// does not open with one.
fn body_under_frontmatter(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    Some(&rest[end + "\n---\n".len()..])
}

/// Whether the file at `path`, called `<stem>.md`, is one purlis generated and may remove: it
/// reads, it is inside the plane and not reached through a link out of it, and its
/// [`authorship`] is purlis's.
pub fn is_generated(root: &Path, path: &Path, stem: &str) -> bool {
    crate::contain::within_plane(root, path)
        && std::fs::read_to_string(path)
            .is_ok_and(|text| authorship(stem, &text) == Authorship::Generated)
}

/// Every file under `.claude/agents/` that purlis generated, by its path in the project, in
/// name order: what the `persona-agents` fix removes, and what the doctor counts.
pub fn generated(root: &Path) -> Vec<String> {
    let dir = agents_dir(root);
    let mut found: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_else(|_| Vec::<PathBuf>::new())
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .filter(|path| {
            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
            std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file())
                && stem.is_some_and(|stem| is_generated(root, path, &stem))
        })
        .map(|path| super::rel(root, &path))
        .collect();
    found.sort();
    found
}

/// Remove the persona `name`'s generated sub-agent, never a hand-written one. What `persona
/// remove` does for the persona it removes, on a project the `persona-agents` fix has not
/// been applied to.
pub(crate) fn remove_agent(root: &Path, name: &str) -> bool {
    let path = agents_dir(root).join(format!("{name}.md"));
    is_generated(root, &path, name) && std::fs::remove_file(&path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_persona_s_description_is_read_along_its_chain_and_agent_description_answers_first() {
        let plane = tempfile::tempdir().unwrap();
        let write = |name: &str, text: &str| {
            let dir = plane.path().join("personas").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("persona.md"), text).unwrap();
        };
        write(
            "base",
            "---\nrole: Base\ndescription: Keeps the lights on\n---\n",
        );
        write("kid", "---\nextends: base\nrole: Kid\n---\n");
        write(
            "own",
            "---\nrole: Own\ndescription: x\nagent-description: Runs nights\n---\n",
        );
        write("bare", "---\nrole: Bare\n---\n");
        assert_eq!(
            description_of(plane.path(), "kid").as_deref(),
            Some("Keeps the lights on")
        );
        assert_eq!(
            description_of(plane.path(), "own").as_deref(),
            Some("Runs nights")
        );
        assert_eq!(description_of(plane.path(), "bare"), None);
        assert_eq!(description_of(plane.path(), "ghost"), None);
    }

    fn generated(marker: &str, name: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: \"The Ops persona.\"\n---\n<!-- {marker} from \
             personas/{name}/persona.md — edit the persona, not this file. -->\n\nThis \
             sub-agent acts as the **{name}** persona — Ops — in an\nisolated context. Adopt \
             the charter below as your role.\n\n# Ops\n"
        )
    }

    #[test]
    fn a_file_in_the_generators_shape_is_purlis_s_under_either_spelling_of_the_marker() {
        for marker in crate::names::SYNC_AGENTS_MARKER.spellings() {
            assert_eq!(
                authorship("ops", &generated(marker, "ops")),
                Authorship::Generated,
                "{marker}"
            );
        }
    }

    #[test]
    fn a_file_without_the_marker_is_hand_written() {
        let own = "---\nname: runner\ndescription: mine\n---\n\nRun the suite.\n";
        assert_eq!(authorship("runner", own), Authorship::HandWritten);
    }

    #[test]
    fn a_marker_that_is_not_where_the_generator_wrote_it_is_somebody_s_edit() {
        let marker = crate::names::SYNC_AGENTS_MARKER.write;
        // Quoted in a hand-written file's prose.
        let quoting = format!("---\nname: guard\n---\n\nNever edit a file that says `{marker}`.\n");
        assert_eq!(authorship("guard", &quoting), Authorship::Edited);
        // A generated file somebody wrote a line above the marker of.
        let above = generated(marker, "ops").replace("---\n<!--", "---\nMy own note.\n<!--");
        assert_eq!(authorship("ops", &above), Authorship::Edited);
        // A generated file copied to another name: it is not that name's sub-agent.
        assert_eq!(
            authorship("ops-copy", &generated(marker, "ops")),
            Authorship::Edited
        );
        // And one with the frontmatter taken off.
        let headless = generated(marker, "ops").replacen("---\n", "", 1);
        assert_eq!(authorship("ops", &headless), Authorship::Edited);
    }

    #[test]
    fn every_retired_key_has_a_sentence_that_names_its_ticket_and_no_other_key_has_one() {
        for key in KEYS {
            let said = key_notice(key, "x").expect(key);
            assert!(said.starts_with(&format!("`{key}:` is no longer read: ")));
            assert!(
                said.contains("(#1460)") || said.contains("(#1453)"),
                "{said}"
            );
        }
        for kept in [
            "model",
            "color",
            "description",
            "agent-description",
            "role",
            "disallowed-tools",
        ] {
            assert_eq!(key_notice(kept, "x"), None, "{kept}");
        }
    }

    #[test]
    fn a_tool_list_says_what_widened_now_that_nothing_reads_it() {
        // D-1451-19: a persona that was read-only by its allow-list is not any more.
        let read_only = key_notice("agent-tools", "Bash, Read, Grep, Glob").unwrap();
        assert!(
            read_only.contains(
                "As a sub-agent this persona could not edit files; as a chat it can, and it \
                 has every other tool of its harness too."
            ),
            "{read_only}"
        );
        let writer = key_notice("agent-tools", "Read, Edit , Bash").unwrap();
        assert!(
            writer.contains(
                "As a sub-agent this persona had only the tools listed; as a chat it has \
                 every tool of its harness."
            ),
            "{writer}"
        );
        assert!(!writer.contains("could not edit files"));
    }

    #[test]
    fn a_model_that_names_no_profile_says_what_the_chat_runs_on_instead() {
        // D-1451-20: a persona that named `haiku` now runs on the asking chat's model.
        let offered = ["claude".to_owned(), "codex".to_owned()];
        let said = model_notice("haiku", false, &offered).unwrap();
        assert!(
            said.contains(
                "A chat as this persona runs on the asking chat's profile, with that \
                 profile's model and its cost, not on `haiku`."
            ),
            "{said}"
        );
        assert_eq!(model_notice("codex", false, &offered), None);
        assert!(
            model_notice("codex", true, &offered)
                .unwrap()
                .ends_with("Delete the line")
        );
    }

    #[test]
    fn the_refusal_names_the_dispatch_route_and_says_a_helper_still_runs() {
        let said = subagent_refusal("devops", false);
        assert!(said.contains("`purlis dispatch --to devops`"), "{said}");
        assert!(said.contains("A helper that is not named for a persona still runs"));
    }

    #[test]
    fn a_draft_s_refusal_does_not_name_a_route_that_is_refused_too() {
        // F4: a dispatch to a draft is refused, so the sentence must not send the chat there.
        let said = subagent_refusal("intern", true);
        assert!(!said.contains("purlis dispatch --to"), "{said}");
        assert!(
            said.contains("still a draft, and a draft persona runs no chat"),
            "{said}"
        );
        assert!(said.contains("A helper that is not named for a persona still runs"));
    }

    #[test]
    fn a_charter_that_still_teaches_the_old_route_is_found_by_its_words() {
        assert_eq!(
            old_route_in("Delegate with `subagent_type: devops`, then run sync-agents."),
            ["subagent_type", "sync-agents"]
        );
        assert!(old_route_in("Dispatch to devops.").is_empty());
    }
}
