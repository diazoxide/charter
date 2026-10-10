//! **The git identity fix** (FX-3): `user.name` and `user.email`, from a name and an email the
//! operator gives, written to git's **global** config.
//!
//! # Why global (D-FX3-2)
//!
//! It is the scope the `git identity` row names: its hint is `git config --global …`, and the
//! row asks git from the project with every scope in force, so a global value is what turns it
//! green. It is also where an identity belongs. Charter commits in the project AND in every
//! clone under `workspaces/`; a value written into the project's own `.git/config` would turn
//! the row green while each clone's commit still failed without a word, which is the silent
//! loss the row exists to name.
//!
//! The write goes through [`crate::worktree::git`], the hardened runner the row reads through:
//! its git keeps only `HOME` from the environment, so the file written is the one the row
//! reads, and a test that hands its child a temporary `HOME` never reaches the real one.
//!
//! # Only what is missing (D-FX3-8)
//!
//! The fix fills in an identity; it never replaces one. Just before it writes, [`apply`] reads
//! the identity in force in the project, the same view the row reads: every scope, asked from
//! the project, with includes and `includeIf` followed. It refuses when both keys are set, and
//! otherwise writes only the unset key(s). The window's form shows a key that is set, locked,
//! from [`current`] (the global config, includes followed).
//!
//! # Not beside an identity picked by folder (D-FX3-9)
//!
//! A global config with any `includeIf.<condition>.path` chooses an identity by where a commit
//! is made. A `[user]` this fix appended would sit after those includes and win in every
//! folder, replacing the identity the operator arranged for each one. So the fix refuses
//! and names what to do instead ([`BY_FOLDER`]).
//!
//! # Checked in the core
//!
//! The input is checked here, field by field, before anything is written ([`Invalid`]), so the
//! window's form and `charter doctor --fix git-identity` refuse the same input with the same
//! words. A refusal writes nothing.

use std::path::Path;

use super::Fixed;

/// What [`super::apply`] says when it is asked for this fix by its id alone.
pub const NEEDS_INPUT: &str = "git-identity needs a name and an email: purlis doctor --fix \
     git-identity --name \"Your Name\" --email you@example.com, or the form behind the Fix \
     button in the window's Doctor";

/// Why the input was refused, field by field: each list is that field's reasons, empty when
/// the field is fine. The shape the window's Settings rows draw under a field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Invalid {
    pub name: Vec<String>,
    pub email: Vec<String>,
}

impl Invalid {
    /// Every reason, each prefixed with its field, as a terminal prints them.
    pub fn lines(&self) -> Vec<String> {
        let name = self.name.iter().map(|why| format!("--name: {why}"));
        let email = self.email.iter().map(|why| format!("--email: {why}"));
        name.chain(email).collect()
    }
}

/// The global identity as git holds it now: each value as git answers it, empty when unset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Current {
    pub name: String,
    pub email: String,
}

impl Current {
    /// Both keys set: there is nothing for this fix to write.
    pub fn complete(&self) -> bool {
        !self.name.is_empty() && !self.email.is_empty()
    }
}

/// The global `user.name` and `user.email`, with the files the global config includes
/// followed, read through the runner the write goes through: what the window's form shows
/// locked.
pub fn current() -> Result<Current, String> {
    let global = |key| {
        ask(
            Path::new("/"),
            &["config", "--global", "--includes", "--get", key],
        )
    };
    Ok(Current {
        name: global("user.name")?,
        email: global("user.email")?,
    })
}

/// The identity a commit in `root` is made with: every scope, includes and `includeIf`
/// followed, asked from `root` — the view the `git identity` row reads (D-FX3-9).
fn in_force(root: &Path) -> Result<Current, String> {
    let get = |key| ask(root, &["config", "--get", key]);
    Ok(Current {
        name: get("user.name")?,
        email: get("user.email")?,
    })
}

/// What [`apply`] refuses when the global config picks an identity by folder (D-FX3-9).
pub const BY_FOLDER: &str = "your global git config picks identities by folder \
     (includeIf); set user.name/user.email yourself";

/// Does the global config, includes followed, carry any `includeIf.<condition>.path`?
fn picks_by_folder() -> Result<bool, String> {
    ask(
        Path::new("/"),
        &[
            "config",
            "--global",
            "--includes",
            "--get-regexp",
            r"^includeif\..*\.path$",
        ],
    )
    .map(|found| !found.is_empty())
}

/// The longest name or email this fix writes, in characters.
pub const LIMIT: usize = 256;

/// Write what is MISSING of git's global identity from `name` and `email` (D-FX3-8).
///
/// **Never over what is there.** The identity in force in `root` — every scope, includes
/// followed, the view the doctor's row reads — is read again immediately before the write, so
/// a dialog opened before the identity was set from a terminal cannot replace it, nor can a
/// `[user]` appended to the global file shadow one an included file holds:
/// - a global config that picks identities by folder (`includeIf`): [`Fixed::Refused`]
///   ([`BY_FOLDER`]), because which identity a commit gets depends on where it is made, and an
///   appended global `[user]` would change that for every folder (D-FX3-9);
/// - both keys set: [`Fixed::Refused`], and nothing is written;
/// - otherwise only the unset key(s) are checked and written. What was given for a key that
///   is set is left out, and said when it differs.
///
/// Each value is trimmed first. `Err` is the input for an unset key refused, field by field,
/// with nothing written. `Ok(Fixed::Ran)` is incomplete when git refused part of the write.
pub fn apply(root: &Path, name: &str, email: &str) -> Result<Fixed, Invalid> {
    let (name, email) = (name.trim(), email.trim());
    let unread = |why: String| {
        Ok(Fixed::Refused(format!(
            "git's identity could not be read, so nothing was written: {why}"
        )))
    };
    match picks_by_folder() {
        Ok(true) => return Ok(Fixed::Refused(BY_FOLDER.to_owned())),
        Ok(false) => {}
        Err(why) => return unread(why),
    }
    let now = match in_force(root) {
        Ok(now) => now,
        Err(why) => return unread(why),
    };
    if now.complete() {
        return Ok(Fixed::Refused(format!(
            "git identity is already set ({} <{}>), so nothing was written",
            crate::shown::line(&now.name),
            crate::shown::line(&now.email)
        )));
    }
    let (write_name, write_email) = (now.name.is_empty(), now.email.is_empty());
    let invalid = Invalid {
        name: write_name
            .then(|| name_refused(name))
            .flatten()
            .into_iter()
            .collect(),
        email: write_email
            .then(|| email_refused(email))
            .flatten()
            .into_iter()
            .collect(),
    };
    if invalid != Invalid::default() {
        return Err(invalid);
    }
    let mut said = Vec::new();
    for (key, given, kept) in [
        ("user.name", name, &now.name),
        ("user.email", email, &now.email),
    ] {
        if !kept.is_empty() && !given.is_empty() && given != kept {
            said.push(format!(
                "• {key} is already set ({}); left as it is",
                crate::shown::line(kept)
            ));
        }
    }
    let mut complete = true;
    let writes = [
        ("user.name", name, write_name),
        ("user.email", email, write_email),
    ];
    for (key, value, _) in writes.into_iter().filter(|(_, _, write)| *write) {
        match set_global(key, value) {
            Ok(()) => said.push(format!(
                "✓ set {key} = {} (global git config)",
                // The name as typed, joiners and all (D-1301-4); the email is never joined.
                if key == "user.name" {
                    shown_name(value)
                } else {
                    crate::shown::line(value)
                }
            )),
            Err(why) => {
                said.push(format!("✗ {key} was not set: {}", crate::shown::line(&why)));
                complete = false;
                // An email without its name is half an identity: the name failed, so stop.
                break;
            }
        }
    }
    Ok(Fixed::Ran { said, complete })
}

/// One `git config` read from `dir`: what it printed, empty when nothing matched (git's
/// exit 1).
fn ask(dir: &Path, args: &[&str]) -> Result<String, String> {
    let run = crate::worktree::git::run(dir, args, crate::doctor::CHECK_TIMEOUT)
        .map_err(|e| e.to_string())?;
    match run.code {
        Some(0) => Ok(run.line().to_owned()),
        Some(1) => Ok(String::new()),
        None => Err(format!(
            "git did not answer within {}s",
            crate::doctor::CHECK_TIMEOUT.as_secs()
        )),
        Some(_) => Err(crate::doctor::first_line(&run.err)),
    }
}

/// `git config --global <key> <value>`, through the hardened runner. Asked from `/`, as the
/// `git` row asks, so no repository's config is read on the way.
fn set_global(key: &str, value: &str) -> Result<(), String> {
    let run = crate::worktree::git::run(
        Path::new("/"),
        &["config", "--global", key, value],
        crate::doctor::CHECK_TIMEOUT,
    )
    .map_err(|e| e.to_string())?;
    match run.code {
        Some(0) => Ok(()),
        None => Err(format!(
            "git did not answer within {}s",
            crate::doctor::CHECK_TIMEOUT.as_secs()
        )),
        Some(_) => Err(crate::doctor::first_line(&run.err)),
    }
}

/// Why `name` (trimmed) cannot be a commit's author name, or `None`.
fn name_refused(name: &str) -> Option<String> {
    if name.is_empty() {
        return Some("Give the name your commits are made under.".into());
    }
    if let Some(why) = secret_refused(name) {
        return Some(why);
    }
    if hides_a_character(name) {
        return Some(
            "A name is one line of visible characters: no control, zero-width or \
             direction-changing characters."
                .into(),
        );
    }
    if name.chars().count() > LIMIT {
        return Some(format!("A name is at most {LIMIT} characters."));
    }
    if !name.chars().any(draws) {
        return Some("A name needs at least one visible character.".into());
    }
    if name.contains(['<', '>']) {
        return Some("A name cannot contain < or >: git keeps those for the email.".into());
    }
    None
}

/// ZERO WIDTH JOINER: joins two emoji into one (a family, a flag, a profession).
const ZWJ: char = '\u{200d}';
/// ZERO WIDTH NON-JOINER: keeps two letters of a joining script apart, as Persian spells
/// a word's parts (the prefix `mi` before a verb).
const ZWNJ: char = '\u{200c}';
/// VARIATION SELECTOR-16, which may follow an emoji before the joiner (the rainbow flag).
const EMOJI_PRESENTATION: char = '\u{fe0f}';

/// Does `name` hold a character [`crate::shown::invisible`] escapes, other than the two joiners
/// a name of a person may need (#1301, D-1301-1, D-1301-3)?
///
/// **Only in a git identity's name, and only where each joiner does its one job:**
/// - a ZWJ between two emoji (after the first's variation selector, if it has one), as in a
///   ZWJ family emoji;
/// - a ZWNJ between two letters of the Arabic script, as Persian writes it;
/// - either, after a letter or a virama of the Brahmic scripts (Devanagari to Malayalam) and
///   before a letter of them: a ZWJ asks for a half form (a Devanagari conjunct, a Malayalam
///   chillu), a ZWNJ keeps the virama shown.
///
/// Anywhere else (at either end, doubled, between two Latin letters, after a digit or a danda)
/// each is refused like every other control, zero-width or direction-changing character.
/// Neither joiner can reorder text or hide a word: each only changes how its two neighbours
/// are drawn. The shared rule, [`crate::shown::invisible`], is not widened: every other writer
/// keeps it as it is.
fn hides_a_character(name: &str) -> bool {
    let chars: Vec<char> = name.chars().collect();
    (0..chars.len()).any(|at| crate::shown::invisible(chars[at]) && !joins(&chars, at))
}

/// Is `chars[at]` a joiner doing its one job between its neighbours ([`hides_a_character`])?
fn joins(chars: &[char], at: usize) -> bool {
    let before = at.checked_sub(1).map(|i| chars[i]);
    let after = chars.get(at + 1).copied();
    let brahmic =
        before.is_some_and(|c| brahmic_letter(c) || virama(c)) && after.is_some_and(brahmic_letter);
    match chars[at] {
        ZWJ => {
            let emoji_before = match before {
                Some(EMOJI_PRESENTATION) => {
                    at.checked_sub(2).is_some_and(|i| pictographic(chars[i]))
                }
                Some(before) => pictographic(before),
                None => false,
            };
            (emoji_before && after.is_some_and(pictographic)) || brahmic
        }
        ZWNJ => (before.is_some_and(arabic_letter) && after.is_some_and(arabic_letter)) || brahmic,
        _ => false,
    }
}

/// `name`, which the name check let through, as the confirmation shows it (#1301, D-1301-4):
/// [`crate::shown::line`], except that a joiner doing its job ([`joins`]) is drawn as itself,
/// so a joined name echoes back as it was typed. Every other invisible character is escaped.
fn shown_name(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len());
    for (at, &c) in chars.iter().enumerate() {
        if crate::shown::invisible(c) && joins(&chars, at) {
            out.push(c);
        } else {
            out.push_str(&crate::shown::one_line(
                c.encode_utf8(&mut [0; 4]),
                crate::shown::NO_CLIP,
            ));
        }
    }
    if out.chars().count() <= crate::shown::DISPLAY_LIMIT {
        return out;
    }
    let mut clipped: String = out.chars().take(crate::shown::DISPLAY_LIMIT).collect();
    clipped.push('…');
    clipped
}

/// Is `c` a letter of the Brahmic scripts' blocks, Devanagari to Malayalam (U+0900-U+0D7F): a
/// consonant, a vowel or a vowel sign, never a digit or a danda.
fn brahmic_letter(c: char) -> bool {
    c.is_alphabetic() && matches!(c as u32, 0x0900..=0x0d7f)
}

/// Is `c` a virama of those scripts: each block's U+0x4D, and Malayalam's vertical-bar and
/// circular viramas.
fn virama(c: char) -> bool {
    let cp = c as u32;
    matches!(cp, 0x0900..=0x0d7f) && (cp & 0x7f == 0x4d || matches!(cp, 0x0d3b | 0x0d3c))
}

/// Is `c` an emoji a ZWJ sequence is made of: the pictographic blocks (symbols, dingbats,
/// arrows and the emoji planes) and the skin-tone modifiers in them. A close reading of
/// Unicode's Extended_Pictographic, without a table of its own.
fn pictographic(c: char) -> bool {
    matches!(
        c as u32,
        0x2300..=0x23ff | 0x2600..=0x27bf | 0x2b00..=0x2bff | 0x1f000..=0x1faff
    )
}

/// Is `c` a letter of the Arabic script, in which Persian, Urdu and others are written: the
/// Arabic blocks and their presentation forms.
fn arabic_letter(c: char) -> bool {
    c.is_alphabetic()
        && matches!(
            c as u32,
            0x0600..=0x06ff | 0x0750..=0x077f | 0x08a0..=0x08ff | 0xfb50..=0xfdff | 0xfe70..=0xfeff
        )
}

/// Characters that take a column and draw nothing in it: the Hangul fillers, letters by
/// category with no glyph of their own (Unicode's Default_Ignorable_Code_Point; U+115F is even
/// two columns wide), and the braille blank, U+2800.
const BLANKS: [char; 5] = ['\u{115f}', '\u{1160}', '\u{2800}', '\u{3164}', '\u{ffa0}'];

/// Does `c` draw something a reader can see (#1250)? Not a space, not one of the characters
/// [`crate::shown::invisible`] escapes, not one that takes no column (a combining or
/// variation mark, a joiner), and not one of the [`BLANKS`]. A name made only of these reads as
/// a blank author.
fn draws(c: char) -> bool {
    use unicode_width::UnicodeWidthChar;
    !c.is_whitespace()
        && !crate::shown::invisible(c)
        && c.width().is_some_and(|w| w > 0)
        && !BLANKS.contains(&c)
}

/// Why `value` cannot be written because it looks like a credential (V91m, D-1250-7), or `None`.
/// The same check every other writer makes, as written and through its escapes, said by the
/// secret's kind and never by its value: a git identity is written to a plain file and into
/// the author line of every commit.
fn secret_refused(value: &str) -> Option<String> {
    crate::secretshape::kind_as_read(None, value).map(|kind| {
        format!(
            "That looks like a secret ({kind}), so nothing was written: an identity is in every \
             commit's author line. Keep the secret in a vault."
        )
    })
}

/// Why `email` (trimmed) is not a plausible email, or `None`. Plausible, not verified: one
/// `@` with something on each side, and nothing git's author line cannot carry.
fn email_refused(email: &str) -> Option<String> {
    if email.is_empty() {
        return Some("Give the email your commits are made under.".into());
    }
    if let Some(why) = secret_refused(email) {
        return Some(why);
    }
    if email
        .chars()
        .any(|c| c.is_whitespace() || crate::shown::invisible(c) || c == '<' || c == '>')
    {
        return Some(
            "An email has no spaces, < or >, and no control, zero-width or direction-changing \
             characters."
                .into(),
        );
    }
    if email.chars().count() > LIMIT {
        return Some(format!("An email is at most {LIMIT} characters."));
    }
    match email.split_once('@') {
        Some((local, domain))
            if !local.is_empty() && !domain.is_empty() && !domain.contains('@') =>
        {
            None
        }
        _ => Some(
            "That does not look like an email: one @ with something on each side, as in \
             you@example.com."
                .into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_or_email_that_spells_a_secret_through_its_escapes_is_refused() {
        let tail = &crate::secretshape::escaped::token()[1..];
        let spelled = format!("\\u0067{tail}");
        for why in [
            name_refused(&spelled),
            email_refused(&format!("{spelled}@x.dev")),
        ] {
            let why = why.expect("refused");
            assert!(why.contains("looks like a secret"), "{why}");
            assert!(!why.contains(tail), "{why}");
        }
    }

    /// #1301 (D-1301-1): a ZWJ family emoji and a Persian name written with a ZWNJ are names,
    /// and each joiner anywhere it does not join is refused like any other invisible character.
    #[test]
    fn a_joiner_that_joins_is_a_name_and_one_that_does_not_is_refused() {
        for name in [
            // man, ZWJ, woman, ZWJ, girl
            "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}",
            // rainbow flag: white flag, VS-16, ZWJ, rainbow
            "Kai \u{1f3f3}\u{fe0f}\u{200d}\u{1f308}",
            // woman, skin tone, ZWJ, laptop
            "Ada \u{1f469}\u{1f3fd}\u{200d}\u{1f4bb}",
            // Persian: mi + ZWNJ + khaham
            "\u{0645}\u{06cc}\u{200c}\u{062e}\u{0648}\u{0627}\u{0647}\u{0645}",
        ] {
            assert_eq!(name_refused(name), None, "{name:?}");
        }
        let invisible = "A name is one line of visible characters: no control, zero-width or \
                         direction-changing characters.";
        for name in [
            // A lone ZWJ at either end.
            "\u{200d}\u{1f468}",
            "\u{1f468}\u{200d}",
            // Two joiners in a row, and a ZWJ between letters.
            "\u{1f468}\u{200d}\u{200d}\u{1f469}",
            "Ad\u{200d}a",
            // A ZWNJ at either end, between Latin letters, or after an emoji.
            "\u{200c}\u{0645}\u{06cc}",
            "\u{0645}\u{06cc}\u{200c}",
            "Ad\u{200c}a",
            "\u{1f468}\u{200c}\u{1f469}",
            // Every other invisible character, joined or not.
            "\u{0645}\u{200b}\u{062e}",
            "\u{1f468}\u{202e}\u{1f469}",
        ] {
            assert_eq!(name_refused(name).as_deref(), Some(invisible), "{name:?}");
        }
    }

    /// #1301 (D-1301-3): the Brahmic scripts join with both joiners too: a ZWJ after a virama
    /// asks for a half form (a Devanagari conjunct), a ZWNJ after one keeps the virama shown.
    /// Each passes only between a letter or virama and a letter of those scripts.
    #[test]
    fn a_devanagari_conjunct_and_a_hindi_name_with_a_non_joiner_are_names() {
        for name in [
            // ksha with a half ka: ka, virama, ZWJ, ssa
            "\u{0915}\u{094d}\u{200d}\u{0937}",
            // Hindi: ka, virama, ZWNJ, ssa, then aa
            "Ra \u{0915}\u{094d}\u{200c}\u{0937}\u{093e}",
            // Malayalam chillu: na, virama, ZWJ, then ta
            "\u{0d28}\u{0d4d}\u{200d}\u{0d24}",
        ] {
            assert_eq!(name_refused(name), None, "{name:?}");
        }
        let invisible = "A name is one line of visible characters: no control, zero-width or \
                         direction-changing characters.";
        for name in [
            // A lone joiner after a virama at the end, or before a Latin letter.
            "\u{0915}\u{094d}\u{200d}",
            "\u{0915}\u{094d}\u{200c}a",
            // Between a Devanagari digit and a letter, and after a danda.
            "\u{0966}\u{200d}\u{0915}",
            "\u{0964}\u{200c}\u{0915}",
            // Every other invisible character between Devanagari letters.
            "\u{0915}\u{200b}\u{0937}",
        ] {
            assert_eq!(name_refused(name).as_deref(), Some(invisible), "{name:?}");
        }
    }

    /// #1301 (D-1301-4): the confirmation draws a joiner the name check let through as itself,
    /// so a joined name echoes back as it was typed, and still escapes every other invisible
    /// character.
    #[test]
    fn a_joined_name_is_shown_as_typed_and_any_other_invisible_character_escaped() {
        for name in [
            "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}",
            "\u{0645}\u{06cc}\u{200c}\u{062e}\u{0648}\u{0627}\u{0647}\u{0645}",
            "\u{0915}\u{094d}\u{200d}\u{0937}",
        ] {
            assert_eq!(shown_name(name), name);
        }
        assert_eq!(shown_name("Ad\u{200d}a"), "Ad\\u200da");
        assert_eq!(shown_name("a\u{202e}b"), "a\\u202eb");
        assert_eq!(
            shown_name(&"\u{0915}".repeat(300)).chars().count(),
            crate::shown::DISPLAY_LIMIT + 1
        );
    }

    /// #1301: the joiners are a name's only. An email keeps the shared rule.
    #[test]
    fn an_email_keeps_refusing_every_joiner() {
        assert!(email_refused("\u{0645}\u{06cc}\u{200c}\u{062e}@x.dev").is_some());
        assert!(email_refused("\u{1f468}\u{200d}\u{1f469}@x.dev").is_some());
    }
}
