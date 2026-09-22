//! charter's OWN documentation pages, for `charter docs list` and `charter docs show`.
//!
//! `charter/docsrc.py`, ported (M2.21). The reason the pages travel with the binary rather
//! than with the plane is that module's and is not repeated here: *a control plane has no
//! reason to vendor a copy and every reason not to — the page a user reads should come from
//! the same install as the behaviour, so `charter docs show secrets` cannot describe a vault
//! the running CLI does not have.*
//!
//! **Why these two are ported rather than refused.** They were clap usage errors, and a usage
//! error is what a Makefile or a wrapper script meets when a Rust `charter` is first on
//! `$PATH` — for a verb the tool being replaced has. M2.11 shipped `docs generate` and scoped
//! these out; the cost of closing the gap turned out to be one directory and one lookup,
//! because [`news`](crate::news) had already built the road: a vendored corpus, `build.rs`
//! compiling it in, and a differential that renders both sides byte for byte.
//!
//! **There is ONE source here, where Python has two.** Python resolves `charter/_docs`
//! (the wheel) first and falls back to the repo's `docs/` for a contributor running from a
//! checkout — and guards that fallback against a stray `docs/` in site-packages belonging to
//! nobody. A charter shipping inside a signed app has no checkout to fall back to, so
//! `build.rs` compiles `docs/` in and [`source`] can never answer `None`. Python's `source()
//! is None` branch — *"reinstall it, the wheel is missing charter/_docs"* — is therefore
//! unreachable here, and [`listing`] states that rather than carrying a message about a build
//! this crate cannot produce: `build.rs` asserts the directory is not empty, so a charter with
//! no pages does not link.
//!
//! **The topic grammar is gone and the property it bought is stronger.** `docsrc._TOPIC` is
//! `^[a-z0-9][a-z0-9._-]*$`, and it exists because `charter docs show ../../etc/passwd` must
//! not be a file-read primitive wearing a documentation command. Here a topic is a key in a
//! table compiled from the crate's own directory: `../../etc/passwd` is not a key, no path is
//! built from the argument, and nothing is opened. The regex would be a second statement of
//! the same refusal with a second chance of being loosened, so it is not written down —
//! `a_topic_that_is_a_path_names_no_page` is what holds the line.

include!(concat!(env!("OUT_DIR"), "/docs_pages.rs"));

/// Where the pages came from, as a sentence `docs list` prints.
///
/// Python prints the DIRECTORY it read them out of, because it has two candidates and which
/// one answered is the thing an operator needs when a page looks wrong. This binary has one,
/// and it is the binary — so the honest answer to "where did this page come from" is the
/// executable that was run, which is also the only actionable half of Python's: it says which
/// install to look at.
///
/// A `current_exe` that cannot be read answers `charter` rather than nothing, because this is
/// a parenthesis in a listing and a listing that fails to print because the kernel would not
/// name its own process is a worse answer than a vaguer one.
pub fn source() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "charter".to_string())
}

/// Every page that can be shown, in topic order.
///
/// Python's `topics()` is `sorted(p.stem for p in root.glob("*.md"))`; the sort happens in
/// `build.rs`, over filenames that differ from their stems by a suffix every one of them
/// shares, so the two orders are the same order.
pub fn topics() -> Vec<&'static str> {
    PAGES.iter().map(|(topic, _)| *topic).collect()
}

/// The page's text, or `None` when `topic` does not name one.
///
/// `None` covers both "no such page" and "not a topic at all", as Python's does: the caller
/// classifies once rather than telling a typo from an escape attempt, which it has no reason
/// to treat differently.
pub fn read(topic: &str) -> Option<&'static str> {
    PAGES
        .iter()
        .find_map(|(name, text)| (*name == topic).then_some(*text))
}

/// `commands.cmd_docs_list`: the topics on stdout, the framing on stderr.
///
/// The split is charter's everywhere, and here it has a caller: `docs list` is what a script
/// pipes to choose a page, so the names go to stdout and the two sentences around them do
/// not. Python flushes stdout before the trailing hint for the same reason — stderr is
/// unbuffered and stdout is not, so without it the hint overtakes the list the moment the
/// output is a pipe. Each line is handed to the caller in order here, so there is nothing to
/// flush between them.
///
/// Python's empty-corpus branch has no counterpart: see the module note.
pub fn listing(say: &mut dyn FnMut(crate::repocmd::Say)) -> u8 {
    use crate::repocmd::Say;

    say(Say::Info(format!("charter documentation ({}):", source())));
    let names: Vec<String> = topics().iter().map(|name| format!("  {name}")).collect();
    // `print(… + "\n")`: the joined block, then the newline `print` adds, then a blank line
    // from the one the join was given. One `Out` per line and a final empty one.
    say(Say::Out(names.join("\n")));
    say(Say::Out(String::new()));
    say(Say::Info(
        "Read one with: charter docs show <topic>".to_string(),
    ));
    0
}

/// `commands.cmd_docs_show`: the page on stdout, or the refusal and the real topics.
///
/// ADR 0009 — classify, don't guess. `docs show persona` is a plausible typo for `personas`
/// and resolving it would be charter deciding what the caller meant; naming the topics lets
/// them decide, and costs one line.
pub fn show(topic: &str, say: &mut dyn FnMut(crate::repocmd::Say)) -> u8 {
    use crate::repocmd::Say;

    let Some(body) = read(topic) else {
        say(Say::Fail(format!(
            "No charter documentation topic named '{topic}'."
        )));
        say(Say::Info(format!("Topics: {}", topics().join(", "))));
        return 1;
    };
    // `print(body, end="" if body.endswith("\n") else "\n")` — one trailing newline, never
    // two. `Say::Out` is a `println!`, so the page's own final newline comes off first.
    say(Say::Out(
        body.strip_suffix('\n').unwrap_or(body).to_string(),
    ));
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repocmd::Say;

    fn spoken(run: impl FnOnce(&mut dyn FnMut(Say)) -> u8) -> (u8, Vec<Say>) {
        let mut lines = Vec::new();
        let code = run(&mut |line| lines.push(line));
        (code, lines)
    }

    #[test]
    fn the_pages_the_binary_carries_are_the_topics_it_offers() {
        let topics = topics();
        assert!(
            topics.contains(&"secrets") && topics.contains(&"personas"),
            "the vendored corpus lost a page charter has always had: {topics:?}"
        );
        let mut sorted = topics.clone();
        sorted.sort_unstable();
        assert_eq!(topics, sorted, "`topics()` is `sorted(...)` in Python");
        assert!(
            topics.iter().all(|t| !t.ends_with(".md")),
            "a topic is a stem, not a filename: {topics:?}"
        );
    }

    /// charter-app#119. The pages that describe `charter init` describe THIS init.
    ///
    /// ADR 0035 and spec decision 27 reversed the default at the top of a git repository, and
    /// these two pages went on printing the old one — `charter init --clone-this-repo`, an
    /// option this binary does not have — because the corpus check held them byte for byte
    /// against a frozen Python charter that still has it. `tests/differential/run.py`'s
    /// `DOCS_DIVERGE` is what lets them be right; this is the cheap half of the same claim,
    /// in the suite a person runs before pushing rather than in the job that takes minutes.
    ///
    /// It asks about the OPTION rather than about a sentence, because the sentence is the
    /// part that is allowed to be rewritten and the option is the part that must never come
    /// back: `--clone-this-repo` in a page charter-app ships is a page telling an operator to
    /// type something that will not run.
    #[test]
    fn no_page_offers_an_init_option_this_charter_does_not_have() {
        for topic in topics() {
            let page = read(topic).expect("a topic the binary offers is a page it carries");
            assert!(
                !page.contains("--clone-this-repo"),
                "docs/{topic}.md offers `charter init --clone-this-repo`, which ADR 0035 \
                 replaced — see DOCS_DIVERGE in tests/differential/run.py"
            );
        }
        for topic in ["control-plane", "install"] {
            let page = read(topic).expect("a page this charter has always shipped");
            assert!(
                page.contains("--plane-is-this-repo"),
                "docs/{topic}.md describes `charter init` in a repository and never names the \
                 opt-in that makes it write anything"
            );
        }
    }

    #[test]
    fn a_topic_that_is_a_path_names_no_page() {
        // `charter docs show ../../etc/passwd` must not be a file read wearing a
        // documentation command. Nothing here builds a path out of the argument at all, and
        // this is the test that says so rather than a grammar that could be loosened.
        for junk in [
            "../../etc/passwd",
            "/etc/passwd",
            "secrets.md",
            "adr/0014",
            "",
            "SECRETS",
        ] {
            assert!(read(junk).is_none(), "{junk} resolved to a page");
        }
    }

    #[test]
    fn a_page_is_printed_with_one_trailing_newline_and_no_more() {
        // charter's `print(body, end="" if body.endswith("\n") else "\n")`. Every vendored
        // page ends in a newline, so the only way to get this wrong is to print the page and
        // a newline of your own — which puts a blank line at the end of every `docs show`.
        let (code, said) = spoken(|say| show("secrets", say));
        assert_eq!(code, 0);
        let [Say::Out(body)] = &said[..] else {
            panic!("a page is one block on stdout, not {said:?}")
        };
        assert!(!body.ends_with('\n'), "Say::Out is a println! already");
        assert_eq!(
            format!("{body}\n"),
            read("secrets").unwrap(),
            "the page printed is not the page vendored"
        );
    }

    #[test]
    fn an_unknown_topic_is_refused_and_the_real_ones_are_named() {
        let (code, said) = spoken(|say| show("persona", say));
        assert_eq!(code, 1);
        assert_eq!(
            said[0],
            Say::Fail("No charter documentation topic named 'persona'.".to_string())
        );
        let Say::Info(topics) = &said[1] else {
            panic!("the topics are the second line: {said:?}")
        };
        assert!(topics.starts_with("Topics: browser, changes, "), "{topics}");
        assert_eq!(said.len(), 2, "and nothing else: {said:?}");
    }

    #[test]
    fn the_listing_puts_the_topics_on_stdout_and_the_framing_on_stderr() {
        let (code, said) = spoken(listing);
        assert_eq!(code, 0);
        let out: Vec<&String> = said
            .iter()
            .filter_map(|line| match line {
                Say::Out(text) => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(
            out.len(),
            2,
            "the block and the blank line `print(… + \"\\n\")` leaves: {said:?}"
        );
        assert!(out[0].starts_with("  browser\n  changes\n"), "{}", out[0]);
        assert!(out[1].is_empty(), "the trailing blank line: {:?}", out[1]);
        assert!(
            matches!(&said[0], Say::Info(text) if text.starts_with("charter documentation (")),
            "{said:?}"
        );
        assert!(
            matches!(said.last(), Some(Say::Info(text))
                     if text == "Read one with: charter docs show <topic>"),
            "{said:?}"
        );
    }
}
