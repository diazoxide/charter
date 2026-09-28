//! A persona's memory upkeep: `charter persona forget`, `dedupe`, `optimize` and `log` — a
//! port of `commands_persona.cmd_persona_forget`, `cmd_persona_dedupe`,
//! `cmd_persona_optimize` and `cmd_persona_log`, on the same store and curation the
//! workspace verbs use ([`crate::memstore`], [`crate::curate`]). And three the Python charter
//! never had — `edit-memory`, `archive-memory` and `unarchive-memory` (ADR 0065) — which call
//! the same [`crate::personas::Persona`] methods the window's memory tab calls.
//!
//! # What was not ported
//!
//! - **`persona migrate`** (legacy `personas/<name>.md` → `<name>/persona.md` + `memory/`).
//!   Every reader here still reads the flat layout ([`crate::personas::def_path`]), so a
//!   plane that never migrated loses nothing; moving one is a `git mv` to
//!   `personas/<name>/persona.md`.
//! - **`persona dispatch-backfill`**, a one-off seeding of the dispatch tally from old
//!   transcripts. The tally has been written live for months; there is nothing left to seed.
//! - **`persona memory-sync`**: a memory goes with the plane's next save (ADR 0051).
//!
//! # What the CLI does around these
//!
//! Which persona is meant when none is named, and saying when a changed store will leave
//! this machine (`[plane] mode`), are the CLI's: each function here returns whether it
//! changed a committed store, and the caller says the rest.

use std::path::{Path, PathBuf};

use crate::memstore;
use crate::repocmd::{Say, Sink};

/// The committed memory store of `owner` — a persona, or `_shared`.
fn memory_dir(root: &Path, owner: &str) -> PathBuf {
    root.join("personas").join(owner).join("memory")
}

/// What `charter persona forget` was asked for.
pub struct Forget<'a> {
    pub name: &'a str,
    pub slug: &'a str,
    /// From the cross-persona `_shared` store rather than the persona's own.
    pub shared: bool,
    /// From this session's scratch rather than the committed store.
    pub ephemeral: bool,
    /// The trace bucket whose scratch `ephemeral` means.
    pub session: &'a str,
}

/// `charter persona forget <name> <slug>`: delete one memory and its index line. The exit code,
/// and whether a committed store changed.
pub fn forget(root: &Path, ask: &Forget, say: Sink) -> (u8, bool) {
    let name = ask.name;
    if let Some(refused) = crate::personas::name_refusal(root, name) {
        say(Say::Fail(refused));
        return (1, false);
    }
    let owner = if ask.shared {
        crate::personas::SHARED
    } else {
        name
    };
    let dir = if ask.ephemeral {
        crate::recall::ephemeral_dir(root, ask.session, owner)
    } else {
        memory_dir(root, owner)
    };
    match memstore::forget(root, &dir, ask.slug) {
        Ok(()) => {
            let kind = if ask.ephemeral {
                "ephemeral"
            } else {
                "persistent"
            };
            say(Say::Done(format!(
                "Forgot '{}' from {name}'s {kind} memory.",
                ask.slug
            )));
            (0, !ask.ephemeral)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            say(Say::Fail(format!("no memory '{}' in that store", ask.slug)));
            (1, false)
        }
        // A slug is one file in the store; `../<elsewhere>` is a path, and the store's resolver
        // is never handed one. Any other refusal — a slug two files' names end in, the index —
        // is the core's sentence, below.
        Err(e)
            if e.kind() == std::io::ErrorKind::InvalidInput
                && !crate::contain::segment_ok(
                    ask.slug.strip_suffix(".md").unwrap_or(ask.slug),
                ) =>
        {
            say(Say::Fail(format!(
                "'{}' is not the slug of one memory — name a file in {}/, not a path.",
                crate::shown::short(ask.slug),
                super::rel(root, &dir)
            )));
            (1, false)
        }
        Err(e) => {
            say(Say::Fail(e.to_string()));
            (1, false)
        }
    }
}

/// What `charter persona edit-memory | archive-memory | unarchive-memory` act on: one memory of
/// a persona's committed store, or of `_shared`'s.
pub struct Memory<'a> {
    pub name: &'a str,
    pub slug: &'a str,
    /// In the cross-persona `_shared` store rather than the persona's own.
    pub shared: bool,
}

impl Memory<'_> {
    /// The store's owner — the persona, or `_shared`.
    fn owner(&self) -> &str {
        if self.shared {
            crate::personas::SHARED
        } else {
            self.name
        }
    }

    /// How a sentence names the store.
    fn place(&self) -> String {
        if self.shared {
            "the shared memory".to_string()
        } else {
            format!("{}'s memory", self.name)
        }
    }

    /// ` --shared` when it is `_shared`'s, for a command a sentence names.
    fn flag(&self) -> &'static str {
        if self.shared { " --shared" } else { "" }
    }

    /// The persona, checked, and the store, or the refusal already said. A slug that is not
    /// one path segment is refused here, before any store is asked, in the words `forget` uses.
    fn persona(&self, root: &Path, say: Sink) -> Option<crate::personas::Persona> {
        if let Some(refused) = crate::personas::name_refusal(root, self.name) {
            say(Say::Fail(refused));
            return None;
        }
        if !crate::contain::segment_ok(self.slug.strip_suffix(".md").unwrap_or(self.slug)) {
            say(Say::Fail(format!(
                "'{}' is not the slug of one memory — name a file in {}/, not a path.",
                crate::shown::short(self.slug),
                super::rel(root, &memory_dir(root, self.owner()))
            )));
            return None;
        }
        match crate::workspaces::Plane::open(root).persona(self.owner()) {
            Ok(persona) => Some(persona),
            Err(e) => {
                say(Say::Fail(e.to_string()));
                None
            }
        }
    }

    /// Say why a memory operation failed; always exit 1.
    fn failed(&self, e: &std::io::Error, say: Sink) -> (u8, bool) {
        if e.kind() == std::io::ErrorKind::NotFound {
            say(Say::Fail(format!(
                "no memory '{}' in {} (list them: charter persona recall {})",
                crate::shown::short(self.slug),
                self.place(),
                self.name
            )));
        } else {
            say(Say::Fail(e.to_string()));
        }
        (1, false)
    }
}

/// `charter persona edit-memory <name> <slug> [--title T] [TEXT] [--shared]`: rewrite one memory
/// in place — the slug kept, the stamp kept, the index line retitled (ADR 0065). What is not
/// given is kept as the file has it. The exit code, and whether a committed store changed.
pub fn edit_memory(
    root: &Path,
    ask: &Memory,
    title: Option<&str>,
    text: Option<&str>,
    say: Sink,
) -> (u8, bool) {
    let Some(persona) = ask.persona(root, say) else {
        return (1, false);
    };
    let exact = match persona.typed_memory(ask.slug, memstore::Typed::Stored) {
        Ok(exact) => exact,
        Err(e) => return ask.failed(&e, say),
    };
    let opened = match persona.open_memory(&exact) {
        Ok(opened) => opened,
        Err(e) => return ask.failed(&e, say),
    };
    let (title, text) = opened.revised(title, text);
    match persona.edit_memory(&exact, &title, &text, memstore::Base::Read(&opened.text)) {
        Ok(path) => {
            say(Say::Done(format!(
                "Edited '{}' in {} → {}",
                ask.slug,
                ask.place(),
                super::rel(root, &path)
            )));
            (0, true)
        }
        Err(memstore::EditRefused::Io(e)) => ask.failed(&e, say),
        Err(stale) => {
            say(Say::Fail(stale.to_string()));
            (1, false)
        }
    }
}

/// `charter persona archive-memory <name> <slug> [--shared]`: move one memory into
/// `memory/archive/` and drop its index line — the window's Delete (ADR 0065).
pub fn archive_memory(root: &Path, ask: &Memory, say: Sink) -> (u8, bool) {
    let Some(persona) = ask.persona(root, say) else {
        return (1, false);
    };
    let exact = match persona.typed_memory(ask.slug, memstore::Typed::Archiving) {
        Ok(exact) => exact,
        Err(e) => return ask.failed(&e, say),
    };
    match persona.archive_memory(&exact) {
        Ok(path) => {
            say(Say::Done(format!(
                "Archived '{}' from {} → {}. Undo: `charter persona unarchive-memory {} {}{}`",
                ask.slug,
                ask.place(),
                super::rel(root, &path),
                ask.name,
                memstore::undo_args(&exact, &path),
                ask.flag()
            )));
            (0, true)
        }
        Err(e) => ask.failed(&e, say),
    }
}

/// `charter persona unarchive-memory <name> <slug> [--as NAME] [--shared]`: move an archived
/// memory back and re-index it — the window's Undo (ADR 0065).
pub fn unarchive_memory(
    root: &Path,
    ask: &Memory,
    restore_as: Option<&str>,
    say: Sink,
) -> (u8, bool) {
    let Some(persona) = ask.persona(root, say) else {
        return (1, false);
    };
    let exact = match persona.typed_memory(ask.slug, memstore::Typed::Restoring) {
        Ok(exact) => exact,
        Err(e) => return ask.failed(&e, say),
    };
    match persona.unarchive_memory(&exact, restore_as) {
        Ok(path) => {
            say(Say::Done(format!(
                "Restored '{}' to {} → {}",
                ask.slug,
                ask.place(),
                super::rel(root, &path)
            )));
            (0, true)
        }
        Err(e) => ask.failed(&e, say),
    }
}

/// `charter persona dedupe [name] [--threshold]`: near-duplicate pairs across the persona's
/// own store and `_shared`, by Jaccard word overlap, for a person to `forget` one. Deletes
/// nothing.
pub fn dedupe(root: &Path, name: &str, threshold: f64, say: Sink) -> u8 {
    if let Some(refused) = crate::personas::name_refusal(root, name) {
        say(Say::Fail(refused));
        return 1;
    }
    let mut entries = Vec::new();
    let mut unread: memstore::Unread = Vec::new();
    for owner in [name, crate::personas::SHARED] {
        let (found, missed) = memstore::read_entries(root, &memory_dir(root, owner));
        entries.extend(found);
        unread.extend(missed);
    }
    for (path, code) in &unread {
        say(Say::Fail(memstore::cannot_check(root, path, *code)));
    }
    let code = u8::from(!unread.is_empty());
    let shown = crate::curate::py_float(threshold);
    let pairs = memstore::duplicates(&entries, threshold);
    if pairs.is_empty() {
        say(Say::Done(format!(
            "no near-duplicate memories for '{name}' (threshold {shown})."
        )));
        return code;
    }
    say(Say::Out(format!(
        "Near-duplicate memory pairs for '{name}' (Jaccard ≥ {shown}):\n"
    )));
    for (score, a, b) in pairs {
        let (a, b) = (&entries[a], &entries[b]);
        say(Say::Out(format!(
            "  {:.0}%  {}\n        ↔ {}\n        {}\n        {}\n",
            score * 100.0,
            a.title,
            b.title,
            super::rel(root, &a.path),
            super::rel(root, &b.path)
        )));
    }
    say(Say::Info(
        "Review, then drop one: charter persona forget <name> <slug> [--shared]".into(),
    ));
    code
}

/// What `charter persona optimize` was asked for.
pub struct Optimize<'a> {
    /// One persona, or `_shared`; `None` for every persona and `_shared`.
    pub name: Option<&'a str>,
    /// `--all`: `_shared` too, even beside a named persona.
    pub all: bool,
    pub apply: bool,
    pub stale_days: i64,
    pub today: chrono::NaiveDate,
}

/// `charter persona optimize`: the curation `workspace optimize` runs, over each persona's
/// committed store and `_shared` — the safe ops with `--apply`, the rest as proposals.
/// `changed` is called once per store `--apply` changed.
pub fn optimize(root: &Path, ask: &Optimize, changed: &mut dyn FnMut(), say: Sink) -> u8 {
    let shared = crate::personas::SHARED;
    let name = ask.name.filter(|n| !n.is_empty());
    // `_shared` is let through by name: it is the one thing besides a persona this reads.
    if let Some(name) = name.filter(|n| *n != shared)
        && let Some(refused) = crate::personas::name_refusal(root, name)
    {
        say(Say::Fail(refused));
        return 1;
    }
    let mut names = match name {
        Some(name) => vec![name.to_string()],
        None => super::names(root),
    };
    if names.is_empty() {
        say(Say::Info("No personas to optimize.".into()));
        return 0;
    }
    if (ask.all || name.is_none()) && !names.iter().any(|n| n == shared) {
        names.push(shared.to_string());
    }
    let stores: Vec<crate::curate::Store> = names
        .iter()
        .map(|n| crate::curate::Store {
            label: n.clone(),
            dir: memory_dir(root, n),
            // Read only for a store that is there: a missing one is skipped by the runner.
            verified_pct: Some(
                memory_dir(root, n)
                    .is_dir()
                    .then(|| {
                        super::stats::row(root, n, super::stats::RECENT_DAYS, ask.today).verify_pct
                    })
                    .flatten(),
            ),
        })
        .collect();
    let how = crate::curate::Optimizing {
        apply: ask.apply,
        stale_days: ask.stale_days,
        today: ask.today,
        proposals: "  proposals (steward: quiz the engineer — not auto-applied):",
        tidy: "\nNo safe ops to apply — corpus is already tidy.",
    };
    crate::curate::optimize(root, &stores, &how, changed, say)
}

/// What `charter persona log` was asked for.
pub struct Log<'a> {
    pub name: &'a str,
    /// Append this note; `None` to show the persona's recent activity.
    pub message: Option<&'a str>,
    /// How many entries to show.
    pub n: i64,
    /// The trace bucket this session writes and reads.
    pub session: &'a str,
    pub now: chrono::NaiveDateTime,
}

/// `charter persona log [name] [message] [-n N]`: note something in this session's activity
/// under the persona, or show its recent activity.
pub fn log(root: &Path, ask: &Log, say: Sink) -> u8 {
    let name = ask.name;
    if let Some(refused) = crate::personas::name_refusal(root, name) {
        say(Say::Fail(refused));
        return 1;
    }
    if let Some(message) = ask.message.filter(|m| !m.is_empty()) {
        crate::trace::record(
            root,
            ask.session,
            "note",
            &[("persona", name), ("msg", message)],
            ask.now,
        );
        say(Say::Done(format!(
            "Noted to {name}'s session activity (see `charter persona log {name}` / `charter \
             persona recall {name}`)."
        )));
        return 0;
    }
    let entries = crate::trace::for_persona(root, ask.session, name, ask.n);
    if entries.is_empty() {
        say(Say::Info(format!(
            "no activity for '{name}' in this session yet."
        )));
        return 0;
    }
    for entry in &entries {
        say(Say::Out(crate::trace::activity_line(entry)));
    }
    0
}

#[cfg(test)]
#[path = "upkeep_tests.rs"]
mod tests;
