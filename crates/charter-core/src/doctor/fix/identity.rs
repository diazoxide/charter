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

/// Check `name` and `email`, then write them as git's global `user.name` and `user.email`.
///
/// Each is trimmed first. `Err` is the input refused, with nothing written; `Ok` is what the
/// write came to: [`Fixed::Ran`], incomplete when git refused part of it, with git's words.
pub fn apply(name: &str, email: &str) -> Result<Fixed, Invalid> {
    let (name, email) = (name.trim(), email.trim());
    let invalid = Invalid {
        name: name_refused(name).into_iter().collect(),
        email: email_refused(email).into_iter().collect(),
    };
    if invalid != Invalid::default() {
        return Err(invalid);
    }
    let mut said = Vec::new();
    let mut complete = true;
    for (key, value) in [("user.name", name), ("user.email", email)] {
        match set_global(key, value) {
            Ok(()) => said.push(format!("✓ set {key} = {value} (global git config)")),
            Err(why) => {
                said.push(format!("✗ {key} was not set: {why}"));
                complete = false;
                // An email without its name is half an identity: the name failed, so stop.
                break;
            }
        }
    }
    Ok(Fixed::Ran { said, complete })
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
    if name.chars().any(char::is_control) {
        return Some("A name is one line, with no control characters.".into());
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
        .any(|c| c.is_whitespace() || c.is_control() || c == '<' || c == '>')
    {
        return Some("An email has no spaces, control characters, < or >.".into());
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
