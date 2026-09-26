//! `charter persona show <name>` — a persona's metadata and its charter, the role an agent
//! actually adopts, with its `extends:` chain applied. A port of
//! `commands_persona.cmd_persona_show` and `_print_memory_summary`.
//!
//! Everything it can print is printed, the charter included, and a memory directory it could
//! not read is counted `?` and named on stderr — never counted as none — with exit 1.

use std::path::Path;

use crate::repocmd::{Say, Sink};

/// `charter persona show <name>`, and its exit code. `session` is the trace bucket whose
/// ephemeral scratch is counted.
pub fn show(root: &Path, state: &Path, name: &str, session: &str, say: Sink) -> u8 {
    if let Some(refused) = crate::personas::name_refusal(root, name) {
        say(Say::Fail(refused));
        return 1;
    }
    let Some(def) = super::resolve(root, name) else {
        say(Say::Fail(format!("persona '{name}' does not load")));
        return 1;
    };
    let out = |text: String, say: Sink| say(Say::Out(text));
    out(
        format!(
            "{} — {}",
            def.get("name").unwrap_or(name),
            def.get("role").unwrap_or_default()
        ),
        say,
    );
    if def.lineage.len() > 1 {
        out(
            format!(
                "inherits: {}  (charter + tools merged below)",
                def.lineage.join(" → ")
            ),
            say,
        );
    }
    if let Some(vault) = super::vault_of(root, state, name) {
        out(
            format!(
                "vault:   {vault}  ({})",
                super::list::vault_status(root, state, Some(&vault))
            ),
            say,
        );
    }
    let tools = crate::personagrant::tools_of(root, name);
    if !tools.is_empty() {
        out(
            format!(
                "tools:   {}  (auto-approved when this persona is active)",
                tools.into_iter().collect::<Vec<_>>().join(", ")
            ),
            say,
        );
    }
    let scripts = crate::personagrant::bin_scripts(root, name);
    if !scripts.is_empty() {
        out(
            format!(
                "scripts: {}  (executables this persona carries — run by path)",
                scripts.into_keys().collect::<Vec<_>>().join(", ")
            ),
            say,
        );
    }
    out(format!("file:    {}", super::def_rel(root, name)), say);
    let unread = memory_summary(root, name, session, say);
    out(String::new(), say);
    out(def.charter, say);
    if unread { 1 } else { 0 }
}

/// The `memory:` lines, and whether any directory could not be read (each named on stderr).
fn memory_summary(root: &Path, name: &str, session: &str, say: Sink) -> bool {
    let personas = root.join("personas");
    let shared = crate::personas::SHARED;
    let mut unread: crate::memstore::Unread = Vec::new();
    let mut count = |dirs: &[std::path::PathBuf]| {
        let mut found = 0;
        let mut missed = false;
        for dir in dirs {
            let (files, not_read) = crate::memstore::read_files(root, dir);
            found += files.len();
            missed |= !not_read.is_empty();
            unread.extend(not_read);
        }
        if missed {
            "?".to_string()
        } else {
            found.to_string()
        }
    };
    let own = count(&[personas.join(name).join("memory")]);
    let shared_count = count(&[personas.join(shared).join("memory")]);
    let ephemeral = count(&[
        crate::recall::ephemeral_dir(root, session, name),
        crate::recall::ephemeral_dir(root, session, shared),
    ]);
    let refs_dir = personas.join(name).join("refs");
    let refs = if crate::contain::readable(root, &refs_dir).is_err() {
        "0".to_string()
    } else {
        match std::fs::read_dir(&refs_dir) {
            Ok(entries) => entries
                .flatten()
                // Every entry but the README, as Python counts them: a `.gitkeep` too.
                .filter(|e| e.file_name() != "README.md")
                .count()
                .to_string(),
            Err(e) if crate::memstore::is_absent(&e) => "0".to_string(),
            Err(e) => {
                unread.push((refs_dir.clone(), e.raw_os_error()));
                "?".to_string()
            }
        }
    };
    say(Say::Out(format!(
        "memory:  {own} own · {shared_count} shared (persistent) · {ephemeral} ephemeral · \
         {refs} refs"
    )));
    say(Say::Out(format!(
        "         personas/{name}/memory/  ·  recall: charter persona recall {name}"
    )));
    for (path, code) in &unread {
        say(Say::Fail(crate::memstore::cannot_check(root, path, *code)));
    }
    !unread.is_empty()
}
