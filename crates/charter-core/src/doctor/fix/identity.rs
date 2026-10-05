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
//! The fix fills in an identity; it never replaces one. [`apply`] reads the global identity
//! again just before it writes, refuses when both keys are set, and otherwise writes only the
//! unset key(s). The window's form shows a key that is set, locked, from [`current`].
//!
//! # Checked in the core
//!
//! The input is checked here, field by field, before anything is written ([`Invalid`]), so the
//! window's form and `charter doctor --fix git-identity` refuse the same input with the same
//! words. A refusal writes nothing.

use std::path::Path;

use super::Fixed;

/// What [`super::apply`] says when it is asked for this fix by its id alone.
pub const NEEDS_INPUT: &str = "git-identity needs a name and an email: charter doctor --fix \
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

/// The global `user.name` and `user.email`, read through the runner the write goes through.
/// What the window's form shows locked, and what [`apply`] re-reads before it writes.
pub fn current() -> Result<Current, String> {
    Ok(Current {
        name: get_global("user.name")?,
        email: get_global("user.email")?,
    })
}

/// The longest name or email this fix writes, in characters.
pub const LIMIT: usize = 256;

/// Write what is MISSING of git's global identity from `name` and `email` (D-FX3-8).
///
/// **Never over what is there.** The global identity is read again immediately before the
/// write, so a dialog opened before the identity was set from a terminal cannot replace it:
/// - both keys set: [`Fixed::Refused`], and nothing is written;
/// - otherwise only the unset key(s) are checked and written. What was given for a key that
///   is set is left out, and said when it differs.
///
/// Each value is trimmed first. `Err` is the input for an unset key refused, field by field,
/// with nothing written. `Ok(Fixed::Ran)` is incomplete when git refused part of the write.
pub fn apply(name: &str, email: &str) -> Result<Fixed, Invalid> {
    let (name, email) = (name.trim(), email.trim());
    let now = match current() {
        Ok(now) => now,
        Err(why) => {
            return Ok(Fixed::Refused(format!(
                "git's global identity could not be read, so nothing was written: {why}"
            )));
        }
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
                crate::shown::line(value)
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

/// `git config --global --get <key>`: its value, empty when it is unset (git's exit 1).
fn get_global(key: &str) -> Result<String, String> {
    let run = crate::worktree::git::run(
        Path::new("/"),
        &["config", "--global", "--get", key],
        crate::doctor::CHECK_TIMEOUT,
    )
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
    if name.chars().any(crate::shown::invisible) {
        return Some(
            "A name is one line of visible characters: no control, zero-width or \
             direction-changing characters."
                .into(),
        );
    }
    if name.chars().count() > LIMIT {
        return Some(format!("A name is at most {LIMIT} characters."));
    }
    if name.contains(['<', '>']) {
        return Some("A name cannot contain < or >: git keeps those for the email.".into());
    }
    None
}

/// Why `email` (trimmed) is not a plausible email, or `None`. Plausible, not verified: one
/// `@` with something on each side, and nothing git's author line cannot carry.
fn email_refused(email: &str) -> Option<String> {
    if email.is_empty() {
        return Some("Give the email your commits are made under.".into());
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
