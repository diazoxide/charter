//! Whether a string may name one entry inside a directory.
//!
//! charter mints its own workspace and persona names, so a value that does not match the
//! alphabet below cannot name one it produced — which is why a name read off disk, out of a
//! manifest, or off the command line is re-checked **before it is joined onto a path**.
//! `Path::join` throws the prefix away when handed an absolute path, and `..` walks out of
//! the plane, so a name that is not checked is a write anywhere on the filesystem.
//!
//! The name rule is deliberately a question about the *string*, never about the disk: asking
//! the filesystem would make a traversal succeed exactly when the attacker's target happens
//! to exist, which is the one case where the answer must not change.
//!
//! # What this does not defend against, on purpose (ADR 0028)
//!
//! **A gate here answers about a path, and it does not hold it.** Every check below is a
//! `stat` walk; the caller then hands the same path, by name, to `fs::write`, `File::open`,
//! `remove_file` or a `git` subprocess. Nothing holds a descriptor across the two. A writer
//! that can change what the name points at, in the window between the answer and the open,
//! gets the open it wants.
//!
//! **Measured, so the next reader does not have to guess at the size of it.** These gates,
//! against a thread planting and removing a symlink at the path, 20,000 rounds each. It is
//! `the_window_each_gate_leaves` in `tests/nothing_escapes_while_a_writer_races.rs`, so it
//! can be run again rather than believed:
//!
//! | gate, and what the caller does next | escapes per 20,000 |
//! | --- | --- |
//! | [`readable`], then `fs::read` — personas, workspaces, memory, `start` | **5025** |
//! | [`no_link_on_the_way`], then `fs::read` — the record read at launch | **1881** |
//! | [`no_link_on_the_way`], then `fs::write` — the record written at quit | **7600** |
//! | [`open_no_link`] / [`create_no_link`], the same two | **0** and **0** |
//!
//! A quarter to two fifths, not a hairline. An earlier pass measured a transcription of these
//! functions into Python and called its counts an upper bound; that was wrong, and wrong in
//! the unsafe direction — the transcription's racer was Python too, so it planted far less
//! often per victim iteration and understated the window by two orders of magnitude. What
//! does not change with the number is who the attacker is, which is the next paragraph.
//!
//! **This is accepted, and the reason is the adversary rather than the cost.** The attacker
//! these gates exist for holds a **commit**, not a process: a committed
//! `workspaces/evil -> ../../elsewhere` travels to every machine that clones the plane
//! (charter #442, #336), and that attack needs no race and is refused whenever the gate
//! looks. A process racing charter on the operator's own machine is a different principal,
//! and charter does not defend against one — `SECURITY.md` says so in those words about the
//! vault guard, which has far more to lose: *"a guard against mistakes, not an attacker with
//! shell access as your user."* Whoever can race `fs::write` can run `cp`. The day something
//! confines an agent below charter's own privilege, ADR 0028 says this is the first decision
//! to re-open.
//!
//! **What is closed is the half charter owns alone.** [`no_link_on_the_way`] refuses *every*
//! link on the way, last component included, **and, since M3, a `..` that walks up out of the
//! root** — which it did not, for the reason its own docstring now carries. Its sharpest
//! callers are the record and the socket under `.charter/app/`, the paths no Python charter
//! writes. For those,
//! [`open_no_link`] and [`create_no_link`] make the kernel answer the link question at the
//! instant of the open. It is the predicate those callers already declare, enforced
//! atomically, with no Python behaviour to diverge from and no measurable cost (9.37 µs
//! against 9.30 µs for the same open without the flag).
//!
//! **What is not closed, at full size.** [`readable`] and [`writable`] — the 5025 row, and
//! the most-used gates — cannot take `O_NOFOLLOW` at all: they deliberately FOLLOW a link
//! that lands back inside the plane, which is Python's `realpath` behaviour and what a plane
//! that links a persona directory depends on. Directory components above the last one are
//! still a check and not a handle, in every gate including the fixed one. Every path handed
//! to `git` is a path, and a descriptor cannot be handed to a subprocess. Doing it properly
//! is a different containment model — "what is beneath this descriptor" rather than "where
//! does this name land" — across the whole core at once, and spec decision 16 puts a
//! security-critical part behind an external review at M3. Deciding its shape here and
//! reviewing it there would be those two steps in the wrong order.

/// The separators no single name may contain, on any platform charter runs on.
///
/// Both, always — not the running platform's. A plane is committed and travels, so a name
/// that is one segment here and two somewhere else is the same defect either way.
const SEPARATORS: [char; 2] = ['/', '\\'];

/// Could `name` name one entry inside some directory?
pub fn segment_ok(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." {
        return false;
    }
    // A NUL terminates the string inside the C library, so the name charter checked and the
    // name the kernel opened would be two different strings.
    if name.contains('\0') {
        return false;
    }
    if name.contains(SEPARATORS) {
        return false;
    }
    // A Windows drive-qualified name (`C:x`) is rooted without starting with a separator.
    if std::path::Path::new(name).is_absolute() || drive_qualified(name) {
        return false;
    }
    true
}

fn drive_qualified(name: &str) -> bool {
    let mut chars = name.chars();
    matches!((chars.next(), chars.next()), (Some(c), Some(':')) if c.is_ascii_alphabetic())
}

/// Why a name charter is about to MINT would not name the same entry on the next machine
/// (charter-app#96).
///
/// Each sentence says what is wrong and what to use instead, because this is a refusal and
/// not a rewrite: charter does not quietly rename a directory the operator will have to live
/// with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Elsewhere {
    #[error(
        "it cannot name one entry in a directory: no '/' or '\\', no '.' or '..', no NUL, and \
         nothing absolute or drive-qualified"
    )]
    NotOneEntry,
    #[error(
        "its stem is a DOS device name — 'con', 'prn', 'aux', 'nul', 'com0'-'com9' or \
         'lpt0'-'lpt9', in any case and whatever follows the first '.'. On Windows 'nul.md' \
         opens the null device, so the write succeeds and the bytes are gone. Put a letter or \
         a digit beside it: 'nul-notes' rather than 'nul'"
    )]
    Device,
    #[error(
        "it ends in '.' or a space, and Windows strips both — 'alpha.' and 'alpha' are one \
         directory there, so a plane naming both has two entries that are one. Drop the last \
         character"
    )]
    Stripped,
    #[error(
        "it holds ':', which opens an alternate data stream on Windows — 'alpha:evil' is \
         content stored inside 'alpha' that no walk over the plane ever lists. Use '-' instead"
    )]
    Stream,
    #[error(
        "it holds a character Windows has no file of that name for: one of '<' '>' '\"' '|' \
         '?' '*', or a control character. Use letters, digits, '.', '_' and '-'"
    )]
    Unspellable,
    #[error(
        "it holds '~', which is the shape of an 8.3 short name ('PROGRA~1') — a name that \
         stands for whatever Windows aliased rather than for itself, and one a shell expands \
         when it leads a path. Use '-' instead"
    )]
    ShortName,
}

/// [`segment_ok`], **and** the name means the same entry on every machine the plane reaches.
///
/// # Why this is a second function and not a tightening of [`segment_ok`]
///
/// A plane is committed and travels — [`SEPARATORS`] says so four lines up, and that is the
/// whole argument for this gate. But the same sentence forbids putting it on the read path:
/// a plane that already holds `workspaces/alpha.` was minted by some charter, and a charter
/// that refuses to LIST it has locked the operator out of their own plane rather than
/// protected them. So the rule goes where a name is first written down and nowhere else, and
/// [`segment_ok`] stays exactly what its own doc comment says it is — *can this string name
/// one entry* — for every caller that is asking about a name it did not choose.
///
/// # The device rule is the rule, not a longer list
///
/// Windows resolves a device by the name's **stem**: everything before the first `.`, with
/// trailing spaces dropped, compared without case. That is why `com1.txt` is the serial port
/// and why a table of literal filenames is the wrong shape — it would have to hold every
/// extension anyone might ever append. The device table itself is `CON PRN AUX NUL`,
/// `COM0`–`COM9` and `LPT0`–`LPT9`, plus the superscript spellings `COM¹ COM² COM³`
/// (U+00B9, U+00B2, U+00B3), which modern Windows folds onto 1, 2 and 3.
///
/// # What this deliberately does not decide
///
/// **Case.** `DevOps` and `devops` are one directory on a case-insensitive filesystem, and
/// that is a rule about a PAIR of names rather than about one — [`persona_name_ok`] already
/// answers it for personas by admitting no capital, and charter-app#94 is where a workspace's
/// answer belongs. Nothing here contradicts either: a name this accepts is still whatever
/// the alphabet beside it says about case.
pub fn mintable(name: &str) -> Result<(), Elsewhere> {
    if !segment_ok(name) {
        return Err(Elsewhere::NotOneEntry);
    }
    if dos_device(name) {
        return Err(Elsewhere::Device);
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(Elsewhere::Stripped);
    }
    if name.contains(':') {
        return Err(Elsewhere::Stream);
    }
    if name.contains(['<', '>', '"', '|', '?', '*']) || name.contains(char::is_control) {
        return Err(Elsewhere::Unspellable);
    }
    if name.contains('~') {
        return Err(Elsewhere::ShortName);
    }
    Ok(())
}

/// Does Windows read `name` as one of its devices?
///
/// The stem — up to the first `.`, trailing spaces dropped — against the device table. See
/// [`mintable`] for why it is spelled as a rule rather than as a list of filenames.
fn dos_device(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ')
        .to_ascii_lowercase();
    if matches!(stem.as_str(), "con" | "prn" | "aux" | "nul") {
        return true;
    }
    // One port number, and only one: `com10` is an ordinary name. The superscripts are here
    // because they are the half of this rule a literal table always misses.
    for port in ["com", "lpt"] {
        if let Some(number) = stem.strip_prefix(port) {
            let mut digits = number.chars();
            if let (Some(c), None) = (digits.next(), digits.next())
                && (c.is_ascii_digit() || matches!(c, '¹' | '²' | '³'))
            {
                return true;
            }
        }
    }
    false
}

/// Can `name` name a workspace this plane contains?
///
/// Containment first, then the alphabet: `^[A-Za-z0-9][A-Za-z0-9._-]*$`. The alphabet is the
/// right rule because charter mints these names itself.
pub fn workspace_name_ok(name: &str) -> bool {
    segment_ok(name) && alphabet_ok(name)
}

/// The one name under `personas/` that is not a persona: the store every persona reads.
pub const SHARED_PERSONA: &str = "_shared";

/// Can `name` name a persona?
///
/// **Lowercase only** — `charter/persona.py:47` is `^[a-z0-9][a-z0-9._-]*$`, a tighter
/// alphabet than a workspace's, and `Alpha` or `DevOps` is not a persona charter would
/// mint. On a case-insensitive filesystem accepting `DevOps` would reach `devops`'s files
/// through a name the plane does not have.
///
/// `_shared` is admitted by name and nothing else is: Python never validates it, reaching
/// that store through a `shared=True` flag instead, so this is where the two models meet.
pub fn persona_name_ok(name: &str) -> bool {
    if name == SHARED_PERSONA {
        return true;
    }
    segment_ok(name) && lowercase_alphabet_ok(name)
}

/// `^[A-Za-z0-9][A-Za-z0-9._-]*$`, hand-rolled rather than pulling in a regex engine for one
/// rule that never changes.
fn alphabet_ok(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// `^[a-z0-9][a-z0-9._-]*$` — a persona's alphabet, which admits no capital.
fn lowercase_alphabet_ok(name: &str) -> bool {
    let lower = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if lower(c) => {}
        _ => return false,
    }
    chars.all(|c| lower(c) || c == '-' || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_name_is_one_entry() {
        assert!(segment_ok("alpha"));
        assert!(segment_ok("my-repo.v2"));
    }

    #[test]
    fn nothing_that_walks_out_of_a_directory_is_one_entry() {
        for name in ["", ".", "..", "../alpha", "a/b", "a\\b", "/abs", "/", "C:x"] {
            assert!(!segment_ok(name), "{name:?} must not name one entry");
        }
    }

    #[test]
    fn a_name_holding_a_nul_is_refused_because_the_kernel_would_see_a_shorter_one() {
        assert!(!segment_ok("alpha\0evil"));
    }

    #[test]
    fn a_workspace_name_is_a_contained_name_in_charters_own_alphabet() {
        assert!(workspace_name_ok("alpha"));
        assert!(workspace_name_ok("alpha.2"));
        assert!(workspace_name_ok("a_b-c"));
        for name in [
            "../escape",
            "/abs",
            "-leading",
            ".hidden",
            "_shared",
            "a b",
            "é",
        ] {
            assert!(
                !workspace_name_ok(name),
                "{name:?} must not name a workspace"
            );
        }
    }

    #[test]
    fn a_persona_name_admits_no_capital_and_only_shared_leads_with_an_underscore() {
        // `charter/persona.py:47` is `^[a-z0-9][a-z0-9._-]*$`. On a case-insensitive
        // filesystem `DevOps` would otherwise reach `devops`'s files under a name the plane
        // does not have.
        assert!(persona_name_ok("devops"));
        assert!(persona_name_ok("dev-ops.2"));
        assert!(
            persona_name_ok("_shared"),
            "admitted by name, and only this one"
        );
        for name in [
            "Alpha",
            "ALPHA",
            "DevOps",
            "CON",
            "_a",
            "__evil",
            "_SHARED",
            "_",
            "A",
            "../escape",
            "/abs",
            "a/b",
            "",
        ] {
            assert!(!persona_name_ok(name), "{name:?} must not name a persona");
        }
    }

    /// Measured against `origin/main` on macOS on 2026-09-22, by calling these functions
    /// rather than by reading them: every one of these was `true`. charter-app#96.
    #[test]
    fn a_name_the_next_machine_reads_as_something_else_is_never_minted() {
        for (name, why) in [
            // A device stem, in the case charter's own alphabet admits…
            ("nul", Elsewhere::Device),
            ("NUL", Elsewhere::Device),
            ("con", Elsewhere::Device),
            ("aux", Elsewhere::Device),
            ("lpt9", Elsewhere::Device),
            ("com0", Elsewhere::Device),
            // …and the same stem with anything at all after the first '.', which is the half
            // a list of literal filenames never covers.
            ("com1.txt", Elsewhere::Device),
            ("nul.md", Elsewhere::Device),
            ("aux .backup", Elsewhere::Device),
            // The superscript ports modern Windows folds onto 1, 2 and 3.
            ("com¹", Elsewhere::Device),
            ("COM³.log", Elsewhere::Device),
            // Stripped at the end: two names, one directory.
            ("alpha.", Elsewhere::Stripped),
            ("alpha ", Elsewhere::Stripped),
            // A stream: content inside `alpha` that a walk over the plane never sees.
            ("alpha:evil", Elsewhere::Stream),
            // An 8.3 alias, and a leading one a shell would expand as well.
            ("PROGRA~1", Elsewhere::ShortName),
            ("~root", Elsewhere::ShortName),
            // Characters Windows has no file for at all.
            ("what?", Elsewhere::Unspellable),
            ("a*b", Elsewhere::Unspellable),
            ("a\u{1}b", Elsewhere::Unspellable),
            // And everything `segment_ok` already refused, under one name.
            ("..", Elsewhere::NotOneEntry),
            ("a/b", Elsewhere::NotOneEntry),
            ("C:x", Elsewhere::NotOneEntry),
            ("", Elsewhere::NotOneEntry),
        ] {
            assert_eq!(mintable(name), Err(why), "{name:?}");
        }
    }

    #[test]
    fn a_name_charter_would_mint_today_still_passes() {
        // The gate is worth nothing if it refuses the names charter actually writes down:
        // every shape in this repo's own plane, plus the ones the device rule sits closest
        // to without touching.
        for name in [
            "alpha",
            "my-repo.v2",
            "a_b-c",
            "charter-app",
            "20260504-113217-the-importer-drops-rows",
            "fixture-session-1",
            "8f14e45f-ceea-467a-9d3f-1b2c3d4e5f60",
            "_shared",
            // Near misses, all ordinary names: a longer port number, a device name with
            // something joined to it, and a stem that only starts like one.
            "com10",
            "com1x",
            "nul-notes",
            "console",
            "auxiliary",
            "lpt",
            "alpha.beta",
        ] {
            assert_eq!(mintable(name), Ok(()), "{name:?} is a name charter mints");
        }
    }

    #[test]
    fn a_plane_that_already_holds_one_of_these_names_can_still_be_read() {
        // The gate is on minting ALONE (charter-app#96). `segment_ok` and the two alphabets
        // are what every reader asks, and a charter that refused to list `workspaces/alpha.`
        // would have locked the operator out of a plane some other charter minted rather
        // than protected them.
        for name in ["nul", "alpha.", "com1.txt", "PROGRA~1"] {
            assert!(
                segment_ok(name),
                "{name:?} must still name one entry to a reader"
            );
        }
        assert!(workspace_name_ok("alpha."));
        assert!(persona_name_ok("nul"));
    }
}

/// Refuses a path that leaves `root`, or is reached through a symlink, walking every
/// component below it.
///
/// **One implementation, because two drift.** `reopen` needs it for the record it keeps under
/// `.charter/app/`, and `hookwire` needs it for the socket it binds in the same directory —
/// and a second, subtly different walk is the failure mode this repo has found five times.
/// What each caller does about its own LEAF (a record must be a plain bounded file; a socket
/// must not be a file at all) stays with that caller. This is only the way there.
///
/// **`root` is named by the caller and never worked out here.** A walk from the root of the
/// filesystem would refuse every path on macOS, where `/tmp` and `/var` are themselves links.
/// The caller passes somewhere it already trusts — the plane, or the runtime directory — and
/// only the components below it are checked.
///
/// Deliberately blunt: these are charter's own paths, created by charter, so a link anywhere
/// on the way has no honest use and there is nothing here to resolve. A component that does
/// not exist yet is fine — charter is about to make it.
///
/// **`..` is refused, and `strip_prefix` is the reason it has to be** (M3). That prefix test
/// is LEXICAL, so `root/.charter/app/../../../../etc/passwd` strips cleanly and the walk
/// then pushes the `..` literally and lets the KERNEL fold it — every component it stats is
/// a real directory, none of them is a link, and the function answered `Ok` for a path
/// nowhere near `root`. `worktree::confine` found that by measuring it
/// (`workspaces/alpha/../../../../etc/passwd` passed) and put a `ParentDir` check in front
/// of its own call; every other caller was safe only because it had validated its own
/// segments first. A function whose name says containment may not depend on that. None of
/// charter's own paths needs a `..`, so there is nothing to lose by refusing one here.
///
/// It is a `stat` and the caller's use is a path open, so a writer racing between the two
/// still wins. That is structural, shared with every gate here, and belongs to one decision
/// about `openat`/`O_NOFOLLOW` across the core rather than to this function.
pub fn no_link_on_the_way(root: &std::path::Path, path: &std::path::Path) -> std::io::Result<()> {
    let Ok(below) = path.strip_prefix(root) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} is not inside {}", path.display(), root.display()),
        ));
    };
    let mut walked = root.to_path_buf();
    for step in below.components() {
        if step == std::path::Component::ParentDir {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "{} walks up out of {}, and charter's own path never does",
                    path.display(),
                    root.display()
                ),
            ));
        }
        walked.push(step);
        match std::fs::symlink_metadata(&walked) {
            Ok(found) if found.file_type().is_symlink() => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!(
                        "{} is a symlink, and charter's own path may not be reached through one",
                        walked.display()
                    ),
                ));
            }
            // Not there yet is fine: charter is about to make it. A component it cannot stat
            // is one it cannot traverse either, so whatever the caller does next fails on the
            // same path a moment later.
            _ => {}
        }
    }
    Ok(())
}

/// [`no_link_on_the_way`], and then the open, with the last component refused by the kernel
/// rather than by charter a moment earlier.
///
/// **Why the pair and not the walk alone.** The walk answers about a path and does not hold
/// it, so a link planted after it has passed is followed by the open — 1881 of 20,000 reads
/// of the record, measured. `O_NOFOLLOW` moves the last component's answer to the instant of
/// the open, where nothing can get between the two.
///
/// **It is the predicate these callers already declare, so nothing changes without an
/// attacker.** [`no_link_on_the_way`] refuses *every* link on the way, the last component
/// included, so an honest path opens exactly as it did. That is why the flag belongs here and
/// **not** on [`readable`] or [`writable`], which deliberately follow a link that lands back
/// inside the plane — Python's `realpath` follows it too, and a plane that links a persona
/// directory depends on it.
///
/// What it does not do: the components ABOVE the last one are still a `stat` and not a
/// handle. ADR 0028 says why the rest waits for M3, and why a directory swap is the harder
/// half (a directory cannot be replaced by a symlink with one `rename`).
pub fn open_no_link(
    root: &std::path::Path,
    path: &std::path::Path,
) -> std::io::Result<std::fs::File> {
    no_link_on_the_way(root, path)?;
    leaf_open(path)
}

/// A file of charter's own read whole: [`open_no_link`], then a plain file or nothing (#440).
///
/// The read half of what `rewrite::replace` does for a write. A link on the way or at the
/// file is refused (`PermissionDenied`), so a secret or a piece of state is never read out of
/// a file that is not the one charter wrote. Something that is not a plain file — a FIFO, a
/// directory — is refused too (`InvalidData`), asked of the descriptor the read uses. A file
/// that is not there is `NotFound`, as `std::fs::read` says it, so a caller's "missing is
/// empty" rule stays exactly as it was.
pub fn read_no_link(root: &std::path::Path, path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut open = open_no_link(root, path)?;
    if !open.metadata()?.file_type().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{} is not a plain file", path.display()),
        ));
    }
    let mut bytes = Vec::new();
    open.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// [`read_no_link`] as UTF-8 text; text that is not UTF-8 is `InvalidData`, worded as
/// `std::fs::read_to_string` words it, since this is what replaces it.
pub fn read_text_no_link(
    root: &std::path::Path,
    path: &std::path::Path,
) -> std::io::Result<String> {
    String::from_utf8(read_no_link(root, path)?).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "stream did not contain valid UTF-8",
        )
    })
}

/// [`open_no_link`]'s twin for a file charter is creating or overwriting.
///
/// `create`/`truncate` and not `create_new`: this replaces `fs::write`, which truncates an
/// existing file, and a temp file left behind by a killed process must not stop the next
/// write.
pub fn create_no_link(
    root: &std::path::Path,
    path: &std::path::Path,
) -> std::io::Result<std::fs::File> {
    no_link_on_the_way(root, path)?;
    leaf_create(path)
}

/// The open half of [`open_no_link`], alone.
///
/// Private, and deliberately reachable only from this module's own tests: on its own it is
/// **not** containment — it says nothing about any component but the last. It is a function
/// rather than two inline `OpenOptions` because a test has to be able to drive the flag
/// without the walk in front of it. Through the pair the walk answers first, so a test that
/// plants a link and calls [`open_no_link`] passes whether or not the flag is there, and
/// would not notice it being dropped.
fn leaf_open(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    nofollow(std::fs::OpenOptions::new().read(true)).open(path)
}

/// [`leaf_open`]'s twin, for a create.
fn leaf_create(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    nofollow(
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true),
    )
    .open(path)
}

/// `O_NOFOLLOW | O_NONBLOCK` on the open, through `rustix` so the constants are not a
/// hand-written table.
///
/// `rustix` is already this crate's dependency under `cfg(unix)`, and `custom_flags` is a
/// safe `std` method, so this needs no new crate and no `unsafe` — which the workspace
/// forbids.
///
/// **`O_NONBLOCK` is here because `O_NOFOLLOW` is not enough on its own.** A FIFO is not a
/// link, so the flag waves it through, and `open`ing one for reading **blocks until a writer
/// appears** — at launch, before there is a window or a tray, leaving an app that can only be
/// killed. Charter's callers answer that by asking the descriptor what it is; they can only
/// ask once the open has returned, which for a FIFO it never does. `O_NONBLOCK` makes it
/// return, and on a regular file — which is all charter's own paths ever are — POSIX gives it
/// no effect at all.
#[cfg(unix)]
pub(crate) fn nofollow(options: &mut std::fs::OpenOptions) -> &mut std::fs::OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    let flags = rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK;
    options.custom_flags(flags.bits() as i32)
}

/// Windows has no `O_NOFOLLOW`; `FILE_FLAG_OPEN_REPARSE_POINT` is the near equivalent and
/// answers differently about a directory junction.
///
/// **Not exercised by any test, deliberately**, for the reason [`resolve_existing`] gives
/// about its `Prefix` arm: the platforms M1 targets cannot drive it. The walk still refuses
/// a link at the last component here, so this is the shipped behaviour minus the atomicity —
/// and it needs its own decision at M4, not a guess now.
#[cfg(not(unix))]
pub(crate) fn nofollow(options: &mut std::fs::OpenOptions) -> &mut std::fs::OpenOptions {
    options
}

#[cfg(test)]
mod nofollow_tests {
    use super::*;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".charter/app")).unwrap();
        dir
    }

    #[test]
    fn an_honest_file_opens_and_reads_back_what_was_written() {
        // The guard against a flag so blunt that ordinary use stops working.
        let dir = plane();
        let file = dir.path().join(".charter/app/reopen.json");

        {
            use std::io::Write;
            let mut out = create_no_link(dir.path(), &file).unwrap();
            out.write_all(b"{}\n").unwrap();
        }
        let mut back = String::new();
        {
            use std::io::Read;
            open_no_link(dir.path(), &file)
                .unwrap()
                .read_to_string(&mut back)
                .unwrap();
        }

        assert_eq!(back, "{}\n");
    }

    /// #440: the whole-file read refuses a link at the file and on the way, and a FIFO; a
    /// missing file is `NotFound` and an honest one reads back whole.
    #[test]
    fn a_whole_file_read_refuses_a_link_and_a_fifo_and_reads_an_honest_file() {
        let dir = plane();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("theirs"), b"THEIRS").unwrap();
        let file = dir.path().join(".charter/app/state");

        assert_eq!(
            read_no_link(dir.path(), &file).unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        std::fs::write(&file, b"ours").unwrap();
        assert_eq!(read_text_no_link(dir.path(), &file).unwrap(), "ours");

        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(outside.path().join("theirs"), &file).unwrap();
        assert!(
            read_no_link(dir.path(), &file).is_err(),
            "a link at the file"
        );

        std::fs::remove_file(&file).unwrap();
        std::fs::remove_dir(dir.path().join(".charter/app")).unwrap();
        std::fs::write(outside.path().join("state"), b"THEIRS").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join(".charter/app")).unwrap();
        assert!(
            read_no_link(dir.path(), &file).is_err(),
            "a link on the way"
        );

        let fifo = dir.path().join(".charter/fifo");
        let made = crate::forklock::status(std::process::Command::new("mkfifo").arg(&fifo))
            .expect("mkfifo runs");
        assert!(made.success(), "a FIFO is made");
        assert_eq!(
            read_no_link(dir.path(), &fifo).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn a_link_planted_after_the_walk_has_passed_is_still_refused_by_the_open() {
        // The property the pair exists for, and the only way to test it: through
        // `open_no_link` the WALK answers first, so planting a link and calling the pair
        // would pass with or without the flag. This drives the open half alone, so it goes
        // red the moment `O_NOFOLLOW` is dropped.
        let dir = plane();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("planted"), b"evil\n").unwrap();
        let file = dir.path().join(".charter/app/reopen.json");

        // The walk passes: nothing is there yet, which it treats as "charter is about to
        // make it".
        no_link_on_the_way(dir.path(), &file).expect("the walk passes before the plant");
        // The attacker gets in between.
        std::os::unix::fs::symlink(outside.path().join("planted"), &file).unwrap();

        let refused = leaf_open(&file).expect_err("the open refuses what the walk could not see");

        // By `ELOOP` and not by something else. `io::ErrorKind::FilesystemLoop` would say it
        // better and is still unstable (`io_error_more`, rust#86442), so the errno is asked
        // for directly — through `rustix`, which already names it, rather than a table of
        // numbers that differ per platform (62 on macOS, 40 on Linux).
        assert_eq!(
            refused.raw_os_error(),
            Some(rustix::io::Errno::LOOP.raw_os_error()),
            "refused by O_NOFOLLOW and not by something else: {refused}"
        );
    }

    #[test]
    fn a_link_planted_after_the_walk_has_passed_is_not_written_through() {
        // The write side of the same property: without the flag this creates the file
        // OUTSIDE the plane and puts the whole record in it.
        let dir = plane();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("planted");
        let file = dir.path().join(".charter/app/reopen.json.writing");

        no_link_on_the_way(dir.path(), &file).expect("the walk passes before the plant");
        std::os::unix::fs::symlink(&target, &file).unwrap();

        leaf_create(&file).expect_err("the create refuses what the walk could not see");

        assert!(
            !target.exists(),
            "nothing was created outside the plane at {}",
            target.display()
        );
    }

    #[test]
    fn a_fifo_swapped_in_after_the_walk_refuses_instead_of_blocking_for_ever() {
        // `O_NOFOLLOW` does not see a FIFO — it is not a link — and opening one for reading
        // blocks until a writer appears, which at launch is an app that can only be killed.
        // `O_NONBLOCK` is what makes the open RETURN so the caller can refuse the
        // descriptor. This test hangs, rather than fails, if that flag is dropped.
        let dir = plane();
        let file = dir.path().join(".charter/app/reopen.json");
        no_link_on_the_way(dir.path(), &file).expect("the walk passes before the plant");
        let made = crate::forklock::status(std::process::Command::new("mkfifo").arg(&file))
            .expect("mkfifo runs");
        assert!(made.success(), "the test needs a fifo to plant");

        // In a thread, because without `O_NONBLOCK` the open never returns at all, and a
        // test that hangs says less than one that fails.
        let (say, heard) = std::sync::mpsc::channel();
        let asked = file.clone();
        std::thread::spawn(move || {
            say.send(
                leaf_open(&asked)
                    .and_then(|open| open.metadata())
                    .map(|found| found.file_type().is_file()),
            )
        });
        let answered = heard
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("opening a fifo must not block");

        assert!(
            !answered.expect("the open returns"),
            "the descriptor says it is not a plain file, which is what the caller refuses on"
        );
    }

    #[test]
    fn the_walk_still_refuses_a_link_at_a_directory_on_the_way() {
        // The half `O_NOFOLLOW` does not cover, kept honest: the pair must still refuse it,
        // through the walk, and this goes red if the walk is dropped for the flag.
        let dir = plane();
        let outside = tempfile::tempdir().unwrap();
        std::fs::remove_dir_all(dir.path().join(".charter/app")).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join(".charter/app")).unwrap();
        let file = dir.path().join(".charter/app/reopen.json");

        let refused = create_no_link(dir.path(), &file)
            .expect_err("a link at a directory on the way is refused");

        assert_eq!(refused.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(
            !outside.path().join("reopen.json").exists(),
            "and nothing was created out there"
        );
    }

    #[test]
    fn a_path_that_walks_up_out_of_the_root_is_refused_with_no_link_anywhere() {
        // **No symlink in this test at all, and that is the point.** `strip_prefix` is
        // lexical, so the `..` survived into the walk, `symlink_metadata` stat'd real
        // directories the whole way down, and the function said `Ok` for `/etc/passwd`. The
        // gate's name promises containment; before M3 it only refused links.
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        // A real file outside the plane, reachable from inside it by name alone. Every
        // directory on the way exists and none of them is a link, which is exactly why the
        // old walk had nothing to object to.
        let outside = tempfile::tempdir().unwrap();
        let outside = std::fs::canonicalize(outside.path()).unwrap();
        std::fs::write(outside.join("loot.json"), "{\"chats\":\"theirs\"}\n").unwrap();
        let up = std::iter::repeat_n("..", root.components().count() + 2)
            .collect::<Vec<_>>()
            .join("/");
        let out = root.join(format!(".charter/app/{up}{}/loot.json", outside.display()));

        let refused = no_link_on_the_way(&root, &out).expect_err("a path that walks up");

        assert_eq!(refused.kind(), std::io::ErrorKind::PermissionDenied);
        // And through the pair, so neither the read nor the write ever happens.
        assert!(open_no_link(&root, &out).is_err());
        assert!(create_no_link(&root, &out).is_err());
        assert_eq!(
            std::fs::read_to_string(outside.join("loot.json")).unwrap(),
            "{\"chats\":\"theirs\"}\n",
            "and the file out there was neither read through nor written over"
        );
    }

    #[test]
    fn a_path_with_no_walk_up_in_it_is_still_allowed() {
        // The guard against a refusal so blunt that charter's own paths stop working —
        // every one of the eighteen callers passes a path built by joining plain names.
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        no_link_on_the_way(&root, &root.join(".charter/app/reopen.json"))
            .expect("charter's own record is not a walk up");
    }
}

/// Where a plane keeps data charter writes. A path that resolves outside all of them is
/// refused, however legal its name.
/// The directories a control plane keeps data in, as `charter/contain.py:data_roots` lists
/// them: `personas/`, `workspaces/` and `.charter/persona-state`.
///
/// **`persona-state` and not `.charter`.** Ephemeral persona memory is data charter is
/// supposed to read and it lives under the secrets home, which is the whole reason this is a
/// list of data directories rather than "the plane, minus `.charter/`". Allowing `.charter`
/// wholesale would put the vaults and every other piece of plane state inside the allowlist.
const DATA_DIRS: [&str; 3] = ["personas", "workspaces", ".charter/persona-state"];

/// A write charter refused, with the reason the operator sees.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Refused {
    #[error(
        "'{path}' resolves to '{resolved}', outside the directories a control plane keeps \
         its data in (persona-state, personas, workspaces). A committed symlink there \
         redirects the {verb}, so charter follows a link that lands inside them and refuses \
         one that leaves"
    )]
    Outside {
        path: String,
        resolved: String,
        /// `read` or `write` — the same refusal, named for what it stopped.
        verb: &'static str,
    },
    /// The link chain was longer than charter follows.
    ///
    /// **A refusal, not a pass.** Giving up used to fall through to "take the name as it
    /// stands", which accepts a path the KERNEL resolves somewhere else entirely — and
    /// Linux's own limit is also 40, so there was no margin in which charter stopped and the
    /// kernel did not. Where charter cannot say where a path lands, it does not write there.
    #[error(
        "'{path}' is behind more than {MAX_LINKS} symlinks, so charter cannot say where the \
         {verb} would land — and will not make one it cannot place"
    )]
    TooManyLinks { path: String, verb: &'static str },
}

/// Check that `path` still lands inside `root`'s data directories once every symlink on the
/// way is followed.
///
/// The name rule alone is not containment. A **committed** symlink at
/// `workspaces/<legal-name>` travels with the plane to every machine that clones it, and
/// pointing it out of the plane redirects every write to that workspace. Python refuses this
/// in `contain.writable`; the name check cannot see it, because the name is fine.
///
/// The path itself need not exist — charter creates workspaces and stores on demand — so the
/// deepest ancestor that DOES exist is resolved and the rest appended. That is the part a
/// symlink can lie about; the remainder is names charter is about to create.
pub fn writable(root: &std::path::Path, path: &std::path::Path) -> Result<(), Refused> {
    contained(root, path, "write")
}

/// The same check before a READ.
///
/// charter gates its reads too, for a reason the write side does not cover: a committed
/// `workspaces/evil -> ../../elsewhere` with a legal name made `workspace vision` PRINT a
/// file from outside the plane (charter #442). Containing the name does not contain that —
/// the name was never the wrong part.
pub fn readable(root: &std::path::Path, path: &std::path::Path) -> Result<(), Refused> {
    contained(root, path, "read")
}

fn contained(
    root: &std::path::Path,
    path: &std::path::Path,
    verb: &'static str,
) -> Result<(), Refused> {
    let too_many = || Refused::TooManyLinks {
        path: path.display().to_string(),
        verb,
    };
    let lands = resolved(path).ok_or_else(too_many)?;
    let base = resolved(root).ok_or_else(too_many)?;
    let inside = DATA_DIRS
        .iter()
        .map(|dir| base.join(dir))
        .any(|allowed| lands.starts_with(&allowed));
    if inside {
        Ok(())
    } else {
        Err(Refused::Outside {
            path: path.display().to_string(),
            resolved: lands.display().to_string(),
            verb,
        })
    }
}

/// Whether `path` resolves anywhere inside `root`, both ends resolved.
///
/// Used where the plane's own state lives rather than its data directories. **Both ends**:
/// on macOS a temp plane is under `/var/folders/...`, itself a link to `/private/var/...`,
/// so comparing a resolved path against an unresolved root refuses everything.
pub fn within_plane(root: &std::path::Path, path: &std::path::Path) -> bool {
    // An exhausted link budget answers NO here too: "charter cannot say where this lands" is
    // not "this is fine".
    match (resolved(path), resolved(root)) {
        (Some(path), Some(root)) => path.starts_with(root),
        _ => false,
    }
}

/// `path` resolved the way the kernel resolves it: left to right, following each symlink as
/// it is reached, and applying `..` to what has been resolved so far.
///
/// **The order is the whole point, and getting it wrong was a live escape.** An earlier
/// version folded `..` lexically BEFORE resolving anything, on the reasoning that folding
/// could only make a path look more escaped than it is. That is backwards. Given
///
/// ```text
/// memory/jump      -> <outside>/inner        (a link out)
/// memory/<name>.md -> jump/../authorized_keys
/// ```
///
/// lexical folding turns `jump/../authorized_keys` into `authorized_keys`, DISCARDING the
/// component that leaves the plane, so the path reads as contained — while the real write
/// follows `jump` out, applies `..`, and creates `<outside>/authorized_keys` with content
/// the caller chose. `..` after a symlink belongs to where the link LANDED, not to the name
/// written before it.
///
/// Python does not have this bug because `contain.within_data` goes through
/// `os.path.realpath`, which resolves component by component. This was a port divergence,
/// which is why a differential scenario now covers it.
///
/// A path that does not exist still resolves: a component with nothing at it is simply
/// appended, so a file charter is about to CREATE is judged where it would land.
///
/// **Public, so that nothing grows a second answer to "where does this path land".** The
/// gates above are one caller; `cistate` is another, comparing a checkout against the key a
/// different process wrote the same checkout under. A second resolver written for that would
/// be a second set of rules about `..` after a link — which is the bug documented above.
///
/// `None` means charter cannot say where the path lands, and every caller treats that as a
/// no rather than as a yes.
pub fn resolved(path: &std::path::Path) -> Option<std::path::PathBuf> {
    use std::path::Component;

    /// One step of a path, owned so a symlink's target can be spliced in.
    enum Step {
        /// A Windows volume or UNC share, which the root after it must not erase.
        Prefix(std::ffi::OsString),
        Root,
        Parent,
        Name(std::ffi::OsString),
    }

    fn steps(path: &std::path::Path) -> Vec<Step> {
        path.components()
            .filter_map(|part| match part {
                // A Windows prefix (`C:`, `\\\\server\\share`) and the root that follows it are
                // two components, and treating both as "reset to this" threw the prefix
                // away — `C:\\x` resolved as if it were `\\x`, which is a different volume.
                //
                // **Not exercised by any test, deliberately.** `Components` yields a
                // `Prefix` only on Windows; on Unix `C:` is an ordinary name, so this arm
                // cannot be driven from the platforms M1 targets. It is written to be right
                // rather than left broken, and stays unverified until Windows at M4 — where
                // it needs a test before it is trusted, not after.
                Component::Prefix(p) => Some(Step::Prefix(p.as_os_str().into())),
                Component::RootDir => Some(Step::Root),
                Component::CurDir => None,
                Component::ParentDir => Some(Step::Parent),
                Component::Normal(n) => Some(Step::Name(n.to_os_string())),
            })
            .collect()
    }

    let mut todo: Vec<Step> = steps(path);
    todo.reverse();
    let mut out = std::path::PathBuf::new();
    let mut links = 0u8;

    while let Some(step) = todo.pop() {
        let name = match step {
            Step::Prefix(prefix) => {
                out = std::path::PathBuf::from(prefix);
                continue;
            }
            Step::Root => {
                // Pushed rather than assigned, so a prefix already in `out` survives.
                out.push(std::path::MAIN_SEPARATOR_STR);
                continue;
            }
            // Pops what has been RESOLVED, which after a link is where the link landed.
            Step::Parent => {
                out.pop();
                continue;
            }
            Step::Name(name) => name,
        };
        let candidate = out.join(&name);
        match std::fs::read_link(&candidate) {
            // The budget is spent BEFORE following, so exhausting it refuses rather than
            // silently accepting the link's own name.
            Ok(_) if links >= MAX_LINKS => return None,
            Ok(target) => {
                links += 1;
                // An absolute target restarts the walk; a relative one continues from the
                // directory the link sits in, which `out` already is.
                if target.is_absolute() {
                    // The target carries its own prefix and root, which its steps re-apply.
                    out = std::path::PathBuf::new();
                }
                // The target's steps come next, BEFORE whatever followed the link — so a
                // `..` after it pops the target, not the link's own name.
                let mut rest = steps(&target);
                rest.reverse();
                todo.extend(rest);
            }
            // Not a link: take the name as it stands.
            Err(_) => out = candidate,
        }
    }
    Some(out)
}

/// How many links deep to follow before giving up, as the kernel does.
const MAX_LINKS: u8 = 40;

#[cfg(test)]
mod writable_tests {
    use super::*;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
        std::fs::create_dir_all(dir.path().join("workspaces")).unwrap();
        dir
    }

    #[test]
    fn a_path_inside_the_workspaces_is_writable() {
        let dir = plane();

        assert_eq!(
            writable(
                dir.path(),
                &dir.path().join("workspaces/alpha/workspace.md")
            ),
            Ok(())
        );
    }

    #[test]
    fn a_workspace_that_is_a_symlink_out_of_the_plane_is_refused() {
        // A COMMITTED symlink travels with the plane, so this is not a local mistake: it
        // redirects every write to that workspace on every machine that clones it.
        let dir = plane();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("workspaces/escape")).unwrap();

        let refusal = writable(
            dir.path(),
            &dir.path().join("workspaces/escape/workspace.md"),
        )
        .expect_err("a link that leaves the plane is refused");

        assert!(
            refusal.to_string().contains("outside the directories"),
            "{refusal}"
        );
    }

    #[test]
    fn a_symlink_that_lands_back_inside_the_plane_is_followed() {
        // charter follows a link that stays inside, and only refuses one that leaves.
        let dir = plane();
        std::fs::create_dir_all(dir.path().join("workspaces/real")).unwrap();
        std::os::unix::fs::symlink(
            dir.path().join("workspaces/real"),
            dir.path().join("workspaces/alias"),
        )
        .unwrap();

        assert_eq!(
            writable(
                dir.path(),
                &dir.path().join("workspaces/alias/workspace.md")
            ),
            Ok(())
        );
    }

    #[test]
    fn a_path_outside_every_data_directory_is_refused() {
        let dir = plane();

        assert!(writable(dir.path(), &dir.path().join("docs/topology.md")).is_err());
        assert!(writable(dir.path(), std::path::Path::new("/etc/passwd")).is_err());
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    fn plane(dir: &std::path::Path) {
        std::fs::write(dir.join("charter.toml"), "schema = 1\n").unwrap();
        std::fs::create_dir_all(dir.join("workspaces/alpha")).unwrap();
    }

    #[test]
    fn a_chain_longer_than_the_budget_is_refused_not_accepted() {
        // Giving up used to fall through to the link's own NAME, which is a path the kernel
        // resolves somewhere else. Linux stops at 40 too, so there is no margin in which
        // charter gives up and the kernel does not.
        let dir = tempfile::tempdir().unwrap();
        plane(dir.path());
        let store = dir.path().join("workspaces/alpha");
        // 45 links, each pointing at the next: longer than MAX_LINKS.
        for i in 0..45 {
            std::os::unix::fs::symlink(format!("link{}", i + 1), store.join(format!("link{i}")))
                .unwrap();
        }

        let refusal = writable(dir.path(), &store.join("link0"))
            .expect_err("a chain charter cannot follow is not a chain it writes through");

        assert!(
            matches!(refusal, Refused::TooManyLinks { .. }),
            "refused for the right reason: {refusal}"
        );
    }

    #[test]
    fn a_chain_within_the_budget_still_resolves() {
        // The guard against a limit so tight that ordinary nesting stops working.
        let dir = tempfile::tempdir().unwrap();
        plane(dir.path());
        let store = dir.path().join("workspaces/alpha");
        for i in 0..20 {
            std::os::unix::fs::symlink(format!("link{}", i + 1), store.join(format!("link{i}")))
                .unwrap();
        }

        assert_eq!(writable(dir.path(), &store.join("link0")), Ok(()));
    }
}

/// Can `name` name a repo cloned into a workspace?
///
/// The same rule as a workspace's, and separate from it on purpose: a repo name arrives from
/// `inventory/repos.json`, written from what a FORGE reported, while a workspace name is one
/// charter minted. They are equal today; the day a forge name needs a wider alphabet, the two
/// must be able to move apart without the other following.
pub fn repo_name_ok(name: &str) -> bool {
    segment_ok(name) && alphabet_ok(name)
}
