//! `charter persona default` — show, set or clear the plane's declared front door.
//!
//! A port of `commands_persona.cmd_persona_default` and of `instance._set_key` beneath it.
//!
//! # Two rungs, and the command writes one of them
//!
//! A plane can declare a front door in two places, and [`crate::active`] reads both:
//! `charter.toml`'s `[persona] default` and the legacy `personas/.default` dotfile.
//! `charter.toml` outranks the dotfile. This command **writes charter.toml** — the dotfile
//! still resolves so a plane that adopted it keeps working, but only one of the two is in the
//! file a consumer opens to read their plane, and the invisible one is the one that shipped,
//! tested green and was adopted by nobody (charter#255).
//!
//! `--clear` removes **both**, or a plane would still declare a front door after being told
//! it no longer does: the dotfile resolves whenever charter.toml is silent.
//!
//! # charter.toml is edited as TEXT
//!
//! `tomllib` reads TOML and cannot write it, and re-emitting through any serialiser strips
//! every comment the file carries — unacceptable in a file people hand-edit. The same is true
//! of Rust's `toml` crate at the value level, so this is the same line-based edit, confined to
//! the section's own line span.
//!
//! **The span is what makes it safe.** `default` is a key under `[workspace]` *and* under
//! `[persona]`, so a file-wide substitution would swap a plane's workspace for a persona
//! name; an earlier Python draft substituted the first `version =` line in the file and
//! happily rewrote a `[[forge]] version = "api-v4"`.
//!
//! **Removal leaves an emptied section header in place.** Deleting it would mean deciding
//! whether a comment sitting under the header belonged to the key or to the section, and an
//! empty `[persona]` reads the same as no `[persona]` to every consumer of this file.

use std::io;
use std::path::Path;

use crate::repocmd::{Say, Sink};

/// `charter persona default [<name>] [--clear]`, and its exit code.
pub fn default_command(root: &Path, name: Option<&str>, clear: bool, say: Sink) -> u8 {
    let manifest = root.join(crate::plane::MANIFEST);
    let legacy = root.join("personas").join(".default");
    if clear {
        // Asked BEFORE the clear, because after it neither rung answers anything.
        let had = declared(root).is_some() || legacy_default(root).is_some();
        // A missing charter.toml declares nothing, so it is the one error that is not a
        // failure here — exactly as a missing `workspaces/.default` is not one for
        // `workspace default --clear`.
        match set_key(&manifest, "persona", "default", None) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => {
                // The OS's own words beside the path, and no cause of charter's (ADR 0009): a
                // read-only file, an unreadable one and a full disk all land here. Nothing
                // about what the file now holds, either: a write that failed after opening
                // can leave it short, so "left as it was" would be a claim unread.
                say(Say::Fail(format!(
                    "could not clear the default persona: could not update {} ({e}).",
                    manifest.display()
                )));
                return 1;
            }
        }
        match std::fs::remove_file(&legacy) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => {
                // An unlink that failed removed nothing, so the file is there as it was; and
                // charter.toml's declaration is already gone, so this dotfile is now the rung
                // that resolves when it names a persona that exists.
                say(Say::Fail(format!(
                    "could not clear the default persona: could not remove {} ({e}), which is \
                     left in place.",
                    legacy.display()
                )));
                return 1;
            }
        }
        if had {
            say(Say::Done(
                "Cleared the declared default persona (commit with `charter save`).".to_string(),
            ));
        } else {
            say(Say::Info("No default persona was declared.".to_string()));
        }
        return 0;
    }

    if let Some(name) = name.filter(|n| !n.is_empty()) {
        if let Some(refused) = crate::personas::name_refusal(root, name) {
            say(Say::Fail(refused));
            return 1;
        }
        match set_key(&manifest, "persona", "default", Some(name)) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // Not the clear's "nothing declared": the declaration is a line in this file,
                // and there is no file to put it in. That is what the OS answered, so it is
                // said as a fact; "could not update" would send the reader after a permission.
                say(Say::Fail(format!(
                    "could not declare the default persona: there is no {} to declare it in.",
                    manifest.display()
                )));
                return 1;
            }
            Err(e) => {
                say(Say::Fail(format!(
                    "could not declare the default persona: could not update {} ({e}).",
                    manifest.display()
                )));
                return 1;
            }
        }
        say(Say::Done(format!(
            "Default persona declared: '{name}' → charter.toml [persona] default (shared; \
             commit with `charter save`)."
        )));
        if legacy.exists() {
            // Said at the moment it becomes true, not in a `doctor` run somebody may never
            // do: from now on the two files disagree and the dotfile is the one that loses.
            say(Say::Warn(format!(
                "personas/.default also exists (naming '{}') and is now IGNORED — charter.toml \
                 outranks it. Delete it: rm personas/.default",
                legacy_default(root).unwrap_or_else(|| "?".to_string())
            )));
        }
        say(Say::Info(
            "Overridden per chat by the persona the app's picker pins, or by $CHARTER_PERSONA."
                .to_string(),
        ));
        return 0;
    }

    // Show. The two rungs in the order they resolve.
    if let Some(cur) = declared(root) {
        say(Say::Out(cur));
        say(Say::Info(
            "declared front door (charter.toml [persona] default). Change: charter persona \
             default <name>  ·  clear: --clear"
                .to_string(),
        ));
        return 0;
    }
    match legacy_default(root) {
        Some(cur) => {
            say(Say::Out(cur.clone()));
            say(Say::Info(format!(
                "committed team-wide default (personas/.default) — the legacy location. Move \
                 it: charter persona default {cur}"
            )));
        }
        None => say(Say::Info(
            "No default persona declared. Set one: charter persona default <name>".to_string(),
        )),
    }
    0
}

/// `[persona] default` as the manifest holds it, **whether or not the persona exists** —
/// `persona.declared_default`.
///
/// Deliberately NOT [`crate::active`]'s rung, which drops a declaration naming a persona the
/// plane no longer defines: `persona default` with no name is the command an operator runs to
/// see WHAT IS WRITTEN, and hiding a stale declaration is how one survives three sessions of
/// somebody wondering why the front door is empty.
fn declared(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join(crate::plane::MANIFEST)).ok()?;
    let doc: toml::Table = text.parse().ok()?;
    let named = doc.get("persona")?.as_table()?.get("default")?.as_str()?;
    let named = crate::memstore::py_strip(named);
    (!named.is_empty()).then(|| named.to_string())
}

/// `personas/.default`, as written — the legacy rung, read for the same reason.
fn legacy_default(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join("personas").join(".default")).ok()?;
    let named = crate::memstore::py_strip(&text);
    (!named.is_empty()).then(|| named.to_string())
}

/// Set — or with `value: None` remove — `key` inside `[section]` of the TOML file at `path`.
///
/// `instance._set_key`, line for line. See this module's header for why it is a text edit and
/// why the edit is confined to the section's own span.
pub fn set_key(path: &Path, section: &str, key: &str, value: Option<&str>) -> io::Result<()> {
    let text = std::fs::read_to_string(path)?;
    // `splitlines(keepends=True)`: every line keeps its own terminator, so a file with no
    // final newline keeps none and one with CRLF keeps it.
    let mut lines = keep_ends(&text);

    // `^[ \t]*\[<section>\][ \t]*$` against the line WITH its ending, which is what Python's
    // `$` allows: the terminator comes off first, then the horizontal whitespace.
    let header_text = format!("[{section}]");
    let is_header = |line: &str| {
        line.trim_end_matches(['\r', '\n'])
            .trim_matches([' ', '\t'])
            == header_text
    };
    let any_header = |line: &str| line.trim_start_matches([' ', '\t']).starts_with('[');
    // `^([ \t]*<key>[ \t]*=[ \t]*).*$` — the prefix is kept so a hand-chosen indent and
    // spacing survive the rewrite.
    let key_prefix = |line: &str| -> Option<usize> {
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if !line[i..].starts_with(key) {
            return None;
        }
        let mut j = i + key.len();
        while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
        }
        if bytes.get(j) != Some(&b'=') {
            return None;
        }
        j += 1;
        while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
        }
        Some(j)
    };

    let start = lines.iter().position(|l| is_header(l));
    match start {
        None => {
            let Some(value) = value else {
                // Nothing declared, nothing to undeclare — and the file is NOT rewritten, so
                // `--clear` on a read-only charter.toml that had nothing to take out of it
                // does not fail (charter#1010).
                return Ok(());
            };
            // `"" if not lines or lines[-1].endswith("\n") else "\n"` — an empty file needs no
            // separator, and one whose last line is unterminated needs exactly one.
            let tail = match lines.last() {
                Some(last) if !last.ends_with('\n') => "\n",
                _ => "",
            };
            lines.push(tail.to_string());
            lines.push("\n".to_string());
            lines.push(format!("[{section}]\n"));
            lines.push(format!("{key} = \"{value}\"\n"));
        }
        Some(start) => {
            let stop = (start + 1..lines.len())
                .find(|i| any_header(&lines[*i]))
                .unwrap_or(lines.len());
            let hit = (start + 1..stop).find(|i| key_prefix(&lines[*i]).is_some());
            match (value, hit) {
                // The emptied header a previous removal leaves behind, and no key: the same
                // "nothing to undeclare" as a file with no section at all.
                (None, None) => return Ok(()),
                (None, Some(at)) => {
                    lines.remove(at);
                }
                (Some(value), None) => {
                    lines.insert(start + 1, format!("{key} = \"{value}\"\n"));
                }
                (Some(value), Some(at)) => {
                    let cut = key_prefix(&lines[at]).expect("matched just above");
                    let ending: String = lines[at]
                        .chars()
                        .rev()
                        .take_while(|c| *c == '\n' || *c == '\r')
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    lines[at] = format!("{}\"{value}\"{ending}", &lines[at][..cut]);
                }
            }
        }
    }
    std::fs::write(path, lines.concat())
}

/// `str.splitlines(keepends=True)` for the subset TOML can hold: `\n` ends a line, and a `\r`
/// in front of it belongs to it.
fn keep_ends(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for c in text.chars() {
        line.push(c);
        if c == '\n' {
            out.push(std::mem::take(&mut line));
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(manifest: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(crate::plane::MANIFEST), manifest).unwrap();
        dir
    }

    fn persona_file(root: &Path, name: &str) {
        std::fs::create_dir_all(root.join("personas").join(name)).unwrap();
        std::fs::write(
            root.join("personas").join(name).join("persona.md"),
            "---\nrole: ops\n---\nbody\n",
        )
        .unwrap();
    }

    fn run(root: &Path, name: Option<&str>, clear: bool) -> (u8, Vec<String>) {
        let mut said = Vec::new();
        let code = default_command(root, name, clear, &mut |line: Say| {
            said.push(line.to_string())
        });
        (code, said)
    }

    fn manifest(root: &Path) -> String {
        std::fs::read_to_string(root.join(crate::plane::MANIFEST)).unwrap()
    }

    #[test]
    fn declaring_writes_charter_toml_and_keeps_every_comment() {
        let dir = plane("# the plane\n[workspace]\ndefault = \"alpha\"  # ours\n");
        persona_file(dir.path(), "devops");

        let (code, said) = run(dir.path(), Some("devops"), false);
        assert_eq!(code, 0, "{said:?}");
        assert_eq!(
            manifest(dir.path()),
            "# the plane\n[workspace]\ndefault = \"alpha\"  # ours\n\n[persona]\ndefault = \"devops\"\n"
        );
        assert!(
            said[0].contains("Default persona declared: 'devops'"),
            "{said:?}"
        );
    }

    #[test]
    fn the_edit_never_leaves_the_sections_own_span() {
        // `default` is a key under BOTH tables. A file-wide substitution would swap the
        // plane's workspace for a persona name.
        let dir = plane("[workspace]\ndefault = \"alpha\"\n\n[persona]\ndefault = \"old\"\n");
        persona_file(dir.path(), "devops");
        run(dir.path(), Some("devops"), false);
        assert_eq!(
            manifest(dir.path()),
            "[workspace]\ndefault = \"alpha\"\n\n[persona]\ndefault = \"devops\"\n"
        );
    }

    #[test]
    fn a_default_in_a_later_section_is_never_the_one_rewritten() {
        // The span, from the other side: `[persona]` comes FIRST here and holds no `default`,
        // so the key is INSERTED under it. Without the span the next `default =` in the file
        // is `[workspace]`'s, and declaring a front door would swap the plane's workspace for
        // a persona name.
        let dir = plane("[persona]\n\n[workspace]\ndefault = \"alpha\"\n");
        persona_file(dir.path(), "devops");
        run(dir.path(), Some("devops"), false);
        assert_eq!(
            manifest(dir.path()),
            "[persona]\ndefault = \"devops\"\n\n[workspace]\ndefault = \"alpha\"\n"
        );
    }

    #[test]
    fn an_existing_key_keeps_its_indent_and_spacing() {
        let dir = plane("[persona]\n  default   =    \"old\"\n");
        persona_file(dir.path(), "devops");
        run(dir.path(), Some("devops"), false);
        assert_eq!(
            manifest(dir.path()),
            "[persona]\n  default   =    \"devops\"\n"
        );
    }

    #[test]
    fn clearing_removes_the_key_and_leaves_the_header() {
        let dir = plane("[persona]\ndefault = \"devops\"\nother = 1\n");
        persona_file(dir.path(), "devops");

        let (code, said) = run(dir.path(), None, true);
        assert_eq!(code, 0, "{said:?}");
        assert_eq!(manifest(dir.path()), "[persona]\nother = 1\n");
        assert!(
            said[0].contains("Cleared the declared default persona"),
            "{said:?}"
        );
    }

    #[test]
    fn clearing_takes_the_legacy_dotfile_with_it() {
        // Or the plane would still declare a front door after being told it no longer does.
        let dir = plane("[persona]\ndefault = \"devops\"\n");
        persona_file(dir.path(), "devops");
        std::fs::write(dir.path().join("personas").join(".default"), "devops\n").unwrap();

        let (code, _said) = run(dir.path(), None, true);
        assert_eq!(code, 0);
        assert!(!dir.path().join("personas/.default").exists());
        assert_eq!(declared(dir.path()), None);
    }

    #[test]
    fn clearing_a_plane_that_declared_nothing_says_so_and_rewrites_nothing() {
        let dir = plane("# only a comment\n");
        let (code, said) = run(dir.path(), None, true);
        assert_eq!(code, 0);
        assert!(
            said[0].contains("No default persona was declared."),
            "{said:?}"
        );
        assert_eq!(manifest(dir.path()), "# only a comment\n");
    }

    #[test]
    fn a_persona_this_plane_does_not_define_is_refused_before_the_file_is_touched() {
        let dir = plane("# the plane\n");
        let (code, said) = run(dir.path(), Some("nope"), false);
        assert_eq!(code, 1);
        assert_eq!(
            said,
            vec!["✗ no persona 'nope' (add it: write personas/nope/persona.md)"]
        );
        assert_eq!(manifest(dir.path()), "# the plane\n");
    }

    #[test]
    fn a_name_that_is_not_one_is_refused_in_the_sentence_every_command_gives() {
        let dir = plane("# the plane\n");
        let (code, said) = run(dir.path(), Some("../../esc"), false);
        assert_eq!(code, 1);
        assert!(said[0].contains("invalid persona name"), "{said:?}");
        assert_eq!(manifest(dir.path()), "# the plane\n");
    }

    #[test]
    fn showing_names_the_rung_that_answered() {
        let dir = plane("# the plane\n");
        persona_file(dir.path(), "devops");

        let (code, said) = run(dir.path(), None, false);
        assert_eq!(code, 0);
        assert!(said[0].contains("No default persona declared."), "{said:?}");

        std::fs::write(dir.path().join("personas").join(".default"), "devops\n").unwrap();
        let (_code, said) = run(dir.path(), None, false);
        assert_eq!(said[0], "devops");
        assert!(said[1].contains("legacy location"), "{said:?}");

        run(dir.path(), Some("devops"), false);
        let (_code, said) = run(dir.path(), None, false);
        assert_eq!(said[0], "devops");
        assert!(
            said[1].contains("charter.toml [persona] default"),
            "{said:?}"
        );
    }

    #[test]
    fn a_stale_declaration_is_still_shown_because_that_is_what_is_written() {
        // `active`'s rung drops it; this command is what an operator runs to see the file.
        let dir = plane("[persona]\ndefault = \"deleted\"\n");
        let (code, said) = run(dir.path(), None, false);
        assert_eq!(code, 0);
        assert_eq!(said[0], "deleted");
    }

    #[test]
    fn declaring_beside_a_legacy_dotfile_says_the_dotfile_now_loses() {
        let dir = plane("# the plane\n");
        persona_file(dir.path(), "devops");
        persona_file(dir.path(), "qa");
        std::fs::write(dir.path().join("personas").join(".default"), "qa\n").unwrap();

        let (code, said) = run(dir.path(), Some("devops"), false);
        assert_eq!(code, 0);
        assert!(
            said.iter()
                .any(|l| l.contains("naming 'qa'") && l.contains("now IGNORED")),
            "{said:?}"
        );
    }

    #[test]
    fn a_file_with_no_final_newline_gains_one_rather_than_a_joined_line() {
        let dir = plane("[workspace]\ndefault = \"alpha\"");
        persona_file(dir.path(), "devops");
        run(dir.path(), Some("devops"), false);
        assert_eq!(
            manifest(dir.path()),
            "[workspace]\ndefault = \"alpha\"\n\n[persona]\ndefault = \"devops\"\n"
        );
    }

    #[test]
    fn declaring_into_a_plane_with_no_manifest_says_there_is_no_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("personas")).unwrap();
        persona_file(dir.path(), "devops");
        let (code, said) = run(dir.path(), Some("devops"), false);
        assert_eq!(code, 1);
        assert!(said[0].contains("there is no"), "{said:?}");
    }
}
