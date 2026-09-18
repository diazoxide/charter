//! The per-file memory store: workspace memory, persona memory and todos all use it.
//!
//! A memory is one Markdown file named after its title, listed in a `MEMORY.md` index
//! beside it. The filename is part of the format — `ws todo done <slug>` closes a todo by
//! its file stem — so the slug rule is reproduced exactly rather than approximated.

/// Is `c` whitespace to Python's `str.strip()`?
///
/// **Not `char::is_whitespace`.** Python drives `strip()` off `str.isspace()`, which is true
/// for the four separator controls U+001C–U+001F; Rust's `White_Space` property is false for
/// all four. The difference is not cosmetic here: `write` refuses an empty body so that a
/// failed substitution cannot land a secret in a memory file, and a body of only U+001F is
/// empty to Python and not to Rust.
pub fn is_python_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{1f}')
}

/// `str.strip()`.
pub fn py_strip(text: &str) -> &str {
    text.trim_matches(is_python_space)
}

/// The longest a memory title may be: it becomes a heading, an index row and part of a
/// filename.
pub const TITLE_MAX: usize = 72;

/// The index every store keeps beside its files.
pub const INDEX: &str = "MEMORY.md";

/// The filename stem charter derives from a title.
///
/// Every run of characters outside `[a-z0-9]` becomes one `-`, the ends are trimmed, and
/// only then is the result cut to 48 characters — so a cut can leave a trailing `-`, and
/// charter keeps it.
pub fn slug(title: &str) -> String {
    let lowered = title.to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    let mut in_run = false;
    for ch in lowered.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
            in_run = false;
        } else if !in_run {
            out.push('-');
            in_run = true;
        }
    }
    let trimmed = out.trim_matches('-');
    // The cut is by characters and comes last, so it can leave the trailing `-` that
    // trimming had just removed from the end of the whole string.
    let cut: String = trimmed.chars().take(48).collect();
    if cut.is_empty() {
        "note".to_string()
    } else {
        cut
    }
}

/// The title `write` derives from a body: its first non-blank line, stripped, capped at 72.
pub fn title_of(text: &str) -> String {
    // One strip, on the line: charter's reader derives the same string, and a second
    // spelling of "first line, stripped, capped" is how the two come to disagree.
    for line in crate::mdsection::split_lines(text) {
        let line = py_strip(line);
        if !line.is_empty() {
            return line.chars().take(TITLE_MAX).collect();
        }
    }
    String::new()
}

/// Create the store directory and its `MEMORY.md` (with `header`) when absent; return the
/// index path.
pub fn ensure_index(dir: &std::path::Path, header: &str) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let index = dir.join(INDEX);
    if !index.exists() {
        let header = if header.ends_with('\n') {
            header.to_string()
        } else {
            format!("{header}\n")
        };
        // Exclusive: `exists()` is false for a dangling link, and a plain write would
        // have created whatever that link named.
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&index)
        {
            Ok(mut f) => std::io::Write::write_all(&mut f, header.as_bytes())?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Ok(index)
}

/// Write one memory file into `dir` and, unless `index` is false, append it to the index.
#[allow(clippy::too_many_arguments)]
pub fn write(
    dir: &std::path::Path,
    text: &str,
    title: Option<&str>,
    timestamped: bool,
    kind: &str,
    index: bool,
    stamp: chrono::NaiveDateTime,
) -> std::io::Result<std::path::PathBuf> {
    let text = py_strip(text);
    if text.is_empty() {
        // charter raises here, and the reason is not tidiness: a secret must never reach a
        // memory file, and an empty body is how a failed substitution arrives.
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "empty memory",
        ));
    }
    let title = match title {
        Some(t) => py_strip(t).chars().take(TITLE_MAX).collect::<String>(),
        None => title_of(text),
    };
    std::fs::create_dir_all(dir)?;

    let prefix = if timestamped {
        stamp.format("%Y%m%d-%H%M%S-").to_string()
    } else {
        String::new()
    };
    let base = slug(&title);
    let mut path = dir.join(format!("{prefix}{base}.md"));
    let mut n = 2;
    // Same title in the same second: keep both, numbered, as charter does.
    while path.exists() {
        path = dir.join(format!("{prefix}{base}-{n}.md"));
        n += 1;
    }

    let body = format!(
        "# {title}\n\n_{} · {kind}_\n\n{text}\n",
        stamp.format("%Y-%m-%d %H:%M")
    );
    std::fs::write(&path, body)?;
    if index {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        index_append(&dir.join(INDEX), &name, &title)?;
    }
    Ok(path)
}

/// Append one `- [title](file)` line. Order is write order: charter never sorts this file.
fn index_append(index: &std::path::Path, filename: &str, title: &str) -> std::io::Result<()> {
    if !index.exists() {
        if let Some(parent) = index.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(index, "# Memory Index\n\n")?;
    }
    let mut f = std::fs::OpenOptions::new().append(true).open(index)?;
    std::io::Write::write_all(&mut f, format!("- [{title}]({filename})\n").as_bytes())
}

/// Find a memory file by its full filename or by a bare slug.
///
/// The direct name wins; otherwise any file whose name ends `-<ident>.md`, which is how a
/// timestamped store is addressed by the slug alone (`ws todo done write-the-migration`).
/// First match in sorted order, as charter's `files()` gives them.
pub fn resolve(dir: &std::path::Path, ident: &str) -> Option<std::path::PathBuf> {
    let name = if ident.ends_with(".md") {
        ident.to_string()
    } else {
        format!("{ident}.md")
    };
    let direct = dir.join(&name);
    if direct.is_file() {
        return Some(direct);
    }
    let suffix = format!("-{name}");
    let mut hits: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().unwrap_or_default().to_string_lossy();
            n != INDEX && (n == name || n.ends_with(&suffix))
        })
        .collect();
    hits.sort();
    hits.into_iter().next()
}

/// Delete one memory and its index line. The index is rewritten as the surviving lines
/// joined with `\n` plus one trailing `\n` — and nothing at all when none survive.
pub fn forget(dir: &std::path::Path, ident: &str) -> std::io::Result<()> {
    let Some(file) = resolve(dir, ident) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no such memory: {ident}"),
        ));
    };
    let slug = file
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    std::fs::remove_file(&file)?;
    let index = dir.join(INDEX);
    if !index.exists() {
        return Ok(());
    }
    let text = std::fs::read_to_string(&index)?;
    let needle = format!("({slug}.md)");
    let kept: Vec<&str> = crate::mdsection::split_lines(&text)
        .into_iter()
        .filter(|line| !line.contains(&needle))
        .collect();
    let body = if kept.is_empty() {
        String::new()
    } else {
        format!("{}\n", kept.join("\n"))
    };
    std::fs::write(&index, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every expectation here came from charter itself: `from charter.memstore import slug`.

    #[test]
    fn a_title_becomes_its_lowercase_words_joined_by_single_dashes() {
        assert_eq!(slug("Hello World"), "hello-world");
        assert_eq!(slug("MiXeD CaSe 123"), "mixed-case-123");
        assert_eq!(
            slug("The API returns 418 on Mondays"),
            "the-api-returns-418-on-mondays"
        );
    }

    #[test]
    fn dashes_at_the_ends_are_trimmed_before_anything_else() {
        assert_eq!(slug("  --Trailing dashes--  "), "trailing-dashes");
    }

    #[test]
    fn a_title_with_nothing_sluggable_in_it_is_called_note() {
        assert_eq!(slug("!!!"), "note");
        assert_eq!(slug(""), "note");
    }

    #[test]
    fn every_non_ascii_letter_is_a_separator_not_a_letter() {
        // Python lowercases first, then replaces anything outside [a-z0-9]; the leading
        // run is trimmed, the inner ones are not.
        assert_eq!(slug("Ünïcödé fäncy"), "n-c-d-f-ncy");
    }

    #[test]
    fn the_cut_to_48_happens_after_trimming_so_it_can_leave_a_trailing_dash() {
        assert_eq!(slug(&"a".repeat(60)), "a".repeat(48));
        assert_eq!(slug(&"x ".repeat(30)), "x-".repeat(24));
    }
}

#[cfg(test)]
mod write_tests {
    use super::*;

    // The expectations here are what `charter.memstore.write` produced in a temp plane with
    // `stamp=datetime(2026, 3, 2, 9, 14)`.

    fn stamp() -> chrono::NaiveDateTime {
        "2026-03-02T09:14:00".parse().unwrap()
    }

    fn store() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("todos");
        ensure_index(
            &store,
            "# Todos — workspace `alpha`\n\nOne line per todo; each links a file holding one thing this task still means to do.\nOpen or done — and done removes it, leaving its trace in the journal instead.\n",
        )
        .unwrap();
        (dir, store)
    }

    #[test]
    fn a_timestamped_memory_is_named_for_its_second_and_its_title() {
        let (_tmp, store) = store();

        let path = write(
            &store,
            "Review the rollout plan",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();

        assert_eq!(
            path.file_name().unwrap(),
            "20260302-091400-review-the-rollout-plan.md"
        );
    }

    #[test]
    fn a_memory_file_is_a_heading_a_stamp_line_and_the_body() {
        let (_tmp, store) = store();

        let path = write(
            &store,
            "Review the rollout plan",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# Review the rollout plan\n\n_2026-03-02 09:14 · persistent_\n\nReview the rollout plan\n"
        );
    }

    #[test]
    fn the_title_of_a_multi_line_body_is_its_first_line_and_the_body_keeps_the_rest() {
        let (_tmp, store) = store();

        let path = write(
            &store,
            "Multi line\nbody here",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();

        assert_eq!(path.file_name().unwrap(), "20260302-091400-multi-line.md");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# Multi line\n\n_2026-03-02 09:14 · persistent_\n\nMulti line\nbody here\n"
        );
    }

    #[test]
    fn two_memories_with_one_title_in_one_second_are_both_kept() {
        let (_tmp, store) = store();

        write(
            &store,
            "Review the rollout plan",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();
        let second = write(
            &store,
            "Review the rollout plan",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();

        assert_eq!(
            second.file_name().unwrap(),
            "20260302-091400-review-the-rollout-plan-2.md"
        );
    }

    #[test]
    fn the_index_gains_one_line_per_memory_in_the_order_they_were_written() {
        let (_tmp, store) = store();

        write(
            &store,
            "Review the rollout plan",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();
        write(
            &store,
            "Review the rollout plan",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();
        write(
            &store,
            "Multi line\nbody here",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(store.join("MEMORY.md")).unwrap(),
            "# Todos — workspace `alpha`\n\nOne line per todo; each links a file holding one thing this task still means to do.\nOpen or done — and done removes it, leaving its trace in the journal instead.\n- [Review the rollout plan](20260302-091400-review-the-rollout-plan.md)\n- [Review the rollout plan](20260302-091400-review-the-rollout-plan-2.md)\n- [Multi line](20260302-091400-multi-line.md)\n"
        );
    }

    #[test]
    fn an_empty_body_is_refused_because_a_secret_must_never_be_written_here() {
        let (_tmp, store) = store();

        assert!(write(&store, "   \n\n ", None, true, "persistent", true, stamp()).is_err());
    }
}
