//! **`persona-agents`** (#1451): what purlis generated for a persona sub-agent, taken out of a
//! project, and the persona keys that fed it carried over.
//!
//! A persona runs as its own chat and is never a harness sub-agent (spec #1434, decision 2), so
//! purlis no longer writes `.claude/agents/<persona>.md`. This is the migration for a project
//! that has those files. It is applied only by its id ([`super::FixId::by_name_only`]): it
//! changes committed files, which every teammate pulls. It makes no commit. What it changed is
//! in the working tree, for the project's next save, and every line of it is said.
//!
//! # What it changes, and nothing else
//!
//! - **It removes each file under `.claude/agents/` that purlis generated and git can give
//!   back.** purlis's file is told by the marker where the generator wrote it
//!   ([`retired::authorship`]). It is removed only where git tracks it and it has no change
//!   that is not committed ([`restorable`]), so `git restore` undoes every removal: a fix
//!   never deletes what could not be brought back. A generated file git does not track, or one
//!   with uncommitted changes, is left and named, as a hand-written file is and one that
//!   carries the marker in any other shape. Claude Code's folder is the only one: purlis never
//!   generated a sub-agent for another harness.
//! - **`model:` becomes `profile:`** in a persona's own definition, where the name is a
//!   profile that travels with the project (a built-in or one the project declares), the
//!   project offers it on this machine, and no definition of its `extends:` chain carries a
//!   `profile:` line: the case in which a chat already starts on that profile
//!   ([`crate::personaprofile`]), so the rewrite changes no chat. A profile that is this
//!   machine's alone is not written into a committed file. Any other `model:` is left where it
//!   is and reported, with what its chats run on instead.
//! - **`color:` is the persona's colour** already ([`crate::personamark`]). Claude Code's two
//!   names for a colour that purlis's palette spells otherwise are rewritten (`cyan` to `teal`,
//!   `magenta` to `pink`); any other value purlis cannot draw is reported.
//!
//! # What it reports and leaves
//!
//! The keys nothing reads now ([`retired::KEYS`]), each with what widened; `disallowed-tools`
//! and a persona's `mcp.json`, with where each holds; the harness's own memory of a retired
//! sub-agent (`.claude/agent-memory/<persona>/`), which nothing reads again. Their lines and
//! files are the operator's.
//!
//! # Safe to run twice
//!
//! Every change is decided from what is on disk. A second run finds no generated file it may
//! remove and no key to rewrite, and says again what it left alone.

use std::path::Path;

use crate::personaverbs::retired::{self, Authorship};

use super::Fixed;

/// Claude Code's names for a sub-agent colour that purlis's palette spells otherwise.
const COLOURS: [(&str, &str); 2] = [("cyan", "teal"), ("magenta", "pink")];

/// The palette's spelling of `colour`, where it is one of Claude Code's names this fix
/// rewrites: what `persona lint` names beside the line.
pub fn rewritten_colour(colour: &str) -> Option<&'static str> {
    COLOURS
        .iter()
        .find(|(theirs, _)| theirs.eq_ignore_ascii_case(colour))
        .map(|(_, ours)| *ours)
}

/// What a run reads of the machine and of git, apart from the project's own files.
pub struct Facts<'a> {
    /// The names of the profiles the project offers on this machine.
    pub offered: &'a [String],
    /// The names of the profiles that travel with the project: the built-ins and the ones it
    /// declares. Only one of these is written into a committed `profile:` line.
    pub travels: &'a [String],
    /// Whether git can give the file at this path (relative to the project) back as it is:
    /// tracked, and with no change that is not committed.
    pub restorable: &'a dyn Fn(&str) -> bool,
}

/// Apply `persona-agents` to the project at `root`. The caller has already refused a project
/// this purlis may not write.
pub fn apply(root: &Path) -> Fixed {
    let offered: Vec<String> = crate::personaprofile::offers(root)
        .into_iter()
        .map(|offer| offer.name)
        .collect();
    let declared = crate::harness_declaration::read(root);
    let travels: Vec<String> = crate::profiles::builtins()
        .into_iter()
        .map(|profile| profile.name)
        .chain(declared.projects().map(|d| d.name.clone()))
        .collect();
    apply_with(
        root,
        &Facts {
            offered: &offered,
            travels: &travels,
            restorable: &|rel| restorable(root, rel),
        },
    )
}

/// Whether git tracks `rel` in the project at `root` and it is as committed: `git status` has
/// nothing to say of it. A project that is no repository, a git that does not answer, an
/// untracked or ignored file and one with changes are all "no", and the file is left.
fn restorable(root: &Path, rel: &str) -> bool {
    use crate::worktree::git;
    let tracked = git::run(root, &["ls-files", "--error-unmatch", "--", rel], git::READ)
        .is_ok_and(|run| run.ok());
    tracked
        && git::run(
            root,
            &["status", "--porcelain=v1", "--ignored", "--", rel],
            git::READ,
        )
        .is_ok_and(|run| run.ok() && run.out.trim().is_empty())
}

/// [`apply`], with what it reads of the machine and of git in hand.
pub fn apply_with(root: &Path, facts: &Facts<'_>) -> Fixed {
    let mut work = Work::default();
    let personas = crate::personaverbs::names(root);
    agents(root, &personas, facts, &mut work);
    for name in &personas {
        definition(root, name, facts, &mut work);
        servers(root, name, &mut work);
    }
    agent_memory(root, &mut work);
    let summary = if work.removed == 0 && work.rewritten == 0 {
        "✓ no generated persona sub-agent file to remove and no persona key to carry over: \
         nothing was changed."
            .to_owned()
    } else {
        format!(
            "✓ removed {} generated sub-agent file(s) and rewrote {} persona key(s). Nothing \
             is committed: the change is in the working tree for the project's next save. To \
             take it back before then: `git restore -- .claude/agents personas`.",
            work.removed, work.rewritten
        )
    };
    work.said.push(summary);
    Fixed::Ran {
        said: work.said,
        complete: work.complete,
    }
}

/// What a run has done and said so far.
struct Work {
    said: Vec<String>,
    removed: usize,
    rewritten: usize,
    complete: bool,
}

impl Default for Work {
    fn default() -> Self {
        Self {
            said: Vec::new(),
            removed: 0,
            rewritten: 0,
            complete: true,
        }
    }
}

impl Work {
    fn failed(&mut self, line: String) {
        self.said.push(line);
        self.complete = false;
    }
}

/// Remove the generated files under `.claude/agents/` that git can give back, and name every
/// other `.md` there.
fn agents(root: &Path, personas: &[String], facts: &Facts<'_>, work: &mut Work) {
    let dir = retired::agents_dir(root);
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    files.sort();
    for file in files {
        let named_md = file
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".md"));
        let Some(stem) = file.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        if !named_md {
            continue;
        }
        let rel = crate::personaverbs::rel(root, &file);
        let shown = crate::shown::short(&rel);
        // A link, or a file reached through one, is not a file purlis wrote here: the
        // generator refused to write through a link too.
        let plain = std::fs::symlink_metadata(&file).is_ok_and(|meta| meta.is_file())
            && crate::contain::within_plane(root, &file);
        if !plain {
            work.said.push(format!(
                "• left alone {shown}: it is a link or is reached through one, and purlis \
                 wrote no such file."
            ));
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            work.said.push(format!(
                "• left alone {shown}: purlis could not read it as text, so it cannot say who \
                 wrote it."
            ));
            continue;
        };
        // What a file that stays means for whoever calls it: a persona has the name.
        let taken = if personas.contains(&stem) {
            format!(
                " It cannot be started while a persona is called `{}`: a sub-agent call to \
                 that name is refused. Rename the file or the persona.",
                crate::shown::short(&stem)
            )
        } else {
            String::new()
        };
        match retired::authorship(&stem, &text) {
            Authorship::Generated if !(facts.restorable)(&rel) => work.said.push(format!(
                "• left alone {shown}: it is a file purlis generated, but git does not track it \
                 or it has changes that are not committed, so removing it could not be undone. \
                 Commit it or remove it yourself, then run this again."
            )),
            Authorship::Generated => match std::fs::remove_file(&file) {
                Ok(()) => {
                    work.removed += 1;
                    work.said
                        .push(format!("✓ removed {shown} (purlis generated it)."));
                }
                Err(why) => work.failed(format!(
                    "✗ could not remove {shown} ({}). It is a file purlis generated; remove it \
                     yourself.",
                    crate::rewrite::os_words(&why)
                )),
            },
            Authorship::Edited => work.said.push(format!(
                "• left alone {shown}: it carries purlis's marker, but not as purlis writes \
                 it, so somebody edited it by hand. Remove it yourself if it is not \
                 wanted.{taken}"
            )),
            Authorship::HandWritten => work.said.push(format!(
                "• left alone {shown}: hand-written (it does not carry purlis's \
                 marker).{taken}"
            )),
        }
    }
}

/// Carry over the keys of `name`'s own definition, and report the ones nothing reads.
fn definition(root: &Path, name: &str, facts: &Facts<'_>, work: &mut Work) {
    let file = crate::personas::def_path(root, name);
    let shown = crate::shown::short(&crate::personaverbs::rel(root, &file));
    let Ok(text) = crate::contain::read_text_no_link(root, &file) else {
        return;
    };
    let pairs = crate::personas::frontmatter(&text);
    let count = |key: &str| pairs.iter().filter(|(k, _)| k == key).count();
    let value = |key: &str| {
        pairs
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.is_empty())
    };
    let twice = |key: &str| {
        format!(
            "`{key}:` is written more than once, so purlis did not choose a line to carry \
             over. `purlis persona lint` says which keys are repeated."
        )
    };
    let mut next = text.clone();
    let mut changed: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    if let Some(model) = value(crate::personaprofile::MODEL_KEY) {
        let chain = crate::personaprofile::chain_names_one(root, name);
        let said = crate::personas::one_line(model);
        if count(crate::personaprofile::MODEL_KEY) > 1 {
            notes.push(twice(crate::personaprofile::MODEL_KEY));
        } else if let Some(notice) = retired::model_notice(model, chain, facts.offered) {
            notes.push(format!("{notice}."));
        } else if !facts.travels.iter().any(|profile| profile == model) {
            // Read as a profile here, and a `profile:` line is committed: a profile that is
            // this machine's alone would be a name a teammate's machine does not offer.
            notes.push(format!(
                "`model: {said}` names a profile that is this machine's alone, so it is left \
                 as it is: a `profile:` line travels with the project. On this machine its \
                 chats start on `{said}`; elsewhere, on the asking chat's profile."
            ));
        } else {
            // It names a profile the project offers, and no `profile:` line of the chain
            // answers instead: what a chat already starts on, written under its own key.
            next = with_line(&next, crate::personaprofile::MODEL_KEY, |_| {
                format!("{}: {model}", crate::personaprofile::KEY)
            });
            changed.push(format!("`model: {said}` is now `profile: {said}`"));
        }
    }

    if count(crate::personamark::COLOR) > 1 {
        notes.push(twice(crate::personamark::COLOR));
    } else if let Some(colour) = value(crate::personamark::COLOR)
        && crate::extension::project::theme::Colour::parse(colour).is_none()
    {
        let said = crate::personas::one_line(colour);
        match rewritten_colour(colour) {
            Some(ours) => {
                next = with_line(&next, crate::personamark::COLOR, |_| {
                    format!("{}: {ours}", crate::personamark::COLOR)
                });
                changed.push(format!("`color: {said}` is now `color: {ours}`"));
            }
            None => notes.push(format!(
                "`color: {said}` is not a colour purlis draws, so this persona keeps the \
                 colour of its name. Use {} or #rrggbb.",
                crate::extension::project::theme::PALETTE.join(", ")
            )),
        }
    }

    for key in retired::KEYS {
        if let Some(said) = value(key)
            && let Some(notice) = retired::key_notice(key, said)
        {
            notes.push(format!("{notice}. The line is left where it is."));
        }
    }
    if value("disallowed-tools").is_some() {
        notes.push(format!("{}.", retired::DENIED_TOOLS));
    }

    if next != text {
        let written = crate::contain::writable(root, &file)
            .map_err(|refused| refused.to_string())
            .and_then(|()| {
                crate::rewrite::replace(root, &file, next.as_bytes(), crate::rewrite::Mode::Kept)
                    .map_err(|why| crate::rewrite::os_words(&why))
            });
        match written {
            Ok(()) => {
                work.rewritten += changed.len();
                for change in changed {
                    work.said.push(format!("✓ {shown}: {change}."));
                }
            }
            Err(why) => work.failed(format!(
                "✗ could not write {shown} ({}). Its keys are as they were.",
                crate::personas::one_line(&why)
            )),
        }
    }
    for note in notes {
        work.said.push(format!("• {shown}: {note}"));
    }
}

/// Report a persona's own `mcp.json`, with where a chat as it is started with its servers. A
/// persona that only inherits them along `extends:` has no file to be told about.
fn servers(root: &Path, name: &str, work: &mut Work) {
    let own = root
        .join("personas")
        .join(name)
        .join(crate::personaverbs::mcp::MCP_FILE);
    let (servers, refused) = crate::personaverbs::mcp::declared(root, name);
    let count = servers.len() + refused.len();
    if count == 0 || !own.is_file() {
        return;
    }
    work.said.push(format!(
        "• personas/{}/{}: {}. `purlis persona lint` says which of them wait for an approval.",
        crate::shown::short(name),
        crate::personaverbs::mcp::MCP_FILE,
        retired::servers_notice(count)
    ));
}

/// Name the harness's own memory of each retired sub-agent: `.claude/agent-memory/<name>/`,
/// which a generated file with `memory:` made Claude Code keep. Nothing reads it again, and
/// nothing here moves or removes it: the notes are somebody's.
fn agent_memory(root: &Path, work: &mut Work) {
    let dir = root.join(".claude").join("agent-memory");
    let mut stores: Vec<(String, usize)> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .map(|entry| {
                    let files = std::fs::read_dir(entry.path())
                        .map(|inside| inside.flatten().count())
                        .unwrap_or(0);
                    (entry.file_name().to_string_lossy().into_owned(), files)
                })
                .filter(|(_, files)| *files > 0)
                .collect()
        })
        .unwrap_or_default();
    stores.sort();
    for (name, files) in stores {
        let name = crate::shown::short(&name);
        work.said.push(format!(
            "• .claude/agent-memory/{name}/ holds {files} file(s) the harness kept for the \
             `{name}` sub-agent. Nothing reads them now. Move what is worth keeping into the \
             persona's own memory with `purlis persona remember {name} \"<fact>\"`; the folder \
             is left as it is."
        ));
    }
}

/// `text`, a persona's definition, with each frontmatter line of `key` replaced by what
/// `line` makes of its value. Every other byte is left as written.
fn with_line(text: &str, key: &str, line: impl Fn(&str) -> String) -> String {
    let Some(rest) = text.strip_prefix("---") else {
        return text.to_owned();
    };
    let Some(end) = rest.find("---") else {
        return text.to_owned();
    };
    let (block, after) = rest.split_at(end);
    let rewritten: String = block
        .split_inclusive('\n')
        .map(|whole| {
            let body = whole.trim_end_matches(['\n', '\r']);
            match body.split_once(':') {
                Some((k, v)) if crate::memstore::py_strip(k) == key => format!(
                    "{}{}",
                    line(crate::memstore::py_strip(v)),
                    &whole[body.len()..]
                ),
                _ => whole.to_owned(),
            }
        })
        .collect();
    format!("---{rewritten}{after}")
}

#[cfg(test)]
#[path = "persona_agents_tests.rs"]
mod tests;
