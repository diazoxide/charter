//! `plugin install`, `plugin`, `plugin files` and `superseded plugin`: whether a chat started
//! outside the app runs charter's hooks (#373, #374).
//!
//! The Python charter's three plugin rows asked about its own Claude Code plugin. This charter
//! arms the chats the app starts by itself, so the same three names now ask about the copy
//! `charter plugin install` puts in front of every other chat on the machine:
//!
//! - **`plugin install`**: is it installed, for each harness set up here?
//! - **`plugin`**: is what is installed what this charter would install now? A copy made by an
//!   older app lacks the newer hooks and skills.
//! - **`plugin files`**: does the `charter` the installed hooks run exist? A hook whose
//!   program is gone cannot start, and a harness reads that as a non-blocking error: the guard
//!   would let every tool call through.
//! - **`superseded plugin`**: does any settings file in force enable the retired Python
//!   charter's `charter@charter`? Both plugins are named `charter`, and a plane that enables
//!   the old one gets both loaded in a terminal chat (measured, Claude Code 2.1.283).
//!
//! Every row reads files and writes none. The repair for the first three is
//! `charter plugin install`, which `charter doctor --fix` runs.

use std::path::PathBuf;

use super::{Doctor, Row, fsx};
use crate::plugin_install::{self as install, Adapter, Machine};

/// The repair, as every hint here says it.
const REPAIR: &str = "`charter plugin install` (or `charter doctor --fix`)";

/// The harnesses set up on this machine: those whose config folder exists.
fn set_up(m: &Machine) -> Vec<&'static dyn Adapter> {
    install::adapters().filter(|a| a.home(m).is_dir()).collect()
}

/// `a`, `a and b`, `a, b and c`.
fn named(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn machine<'a>(d: &'a Doctor, name: &str) -> Result<&'a Machine, Row> {
    d.machine.as_ref().ok_or_else(|| {
        Row::not_checked(
            name,
            "charter cannot tell where Claude Code and Codex keep their configuration",
        )
    })
}

/// The harnesses charter's plugin is installed for, or the row that says why nobody can tell.
fn installed(m: &Machine, name: &str) -> Result<Vec<&'static dyn Adapter>, Row> {
    let mut out = Vec::new();
    for a in set_up(m) {
        match a.installed(m) {
            Ok(true) => out.push(a),
            Ok(false) => {}
            Err(why) => return Err(Row::not_checked(name, super::one_line(&why, 1024))),
        }
    }
    Ok(out)
}

pub(super) fn plugin_install(d: &Doctor) -> Row {
    const NAME: &str = "plugin install";
    let m = match machine(d, NAME) {
        Ok(m) => m,
        Err(row) => return row,
    };
    let here = set_up(m);
    if here.is_empty() {
        return Row::ok(
            NAME,
            "neither Claude Code nor Codex has a config folder on this machine",
        );
    }
    let on = match installed(m, NAME) {
        Ok(on) => on,
        Err(row) => return row,
    };
    let missing: Vec<String> = here
        .iter()
        .filter(|a| !on.iter().any(|b| b.harness() == a.harness()))
        .map(|a| a.harness().to_owned())
        .collect();
    if missing.is_empty() {
        let names: Vec<String> = on.iter().map(|a| a.harness().to_owned()).collect();
        return Row::ok(NAME, format!("installed for {}", named(&names)));
    }
    Row::warn(
        NAME,
        format!(
            "not installed for {} — a chat started outside the app runs without charter's \
             hooks and guard",
            named(&missing)
        ),
        format!("{REPAIR} installs it. The chats the app starts are armed by the app either way."),
    )
}

pub(super) fn plugin(d: &Doctor) -> Row {
    const NAME: &str = "plugin";
    let m = match machine(d, NAME) {
        Ok(m) => m,
        Err(row) => return row,
    };
    let on = match installed(m, NAME) {
        Ok(on) => on,
        Err(row) => return row,
    };
    if on.is_empty() {
        return Row::ok(NAME, "nothing installed to compare");
    }
    let mut stale: Vec<String> = Vec::new();
    for a in &on {
        match a.install(m) {
            Ok(plan) if plan.changes() => stale.push(a.harness().to_owned()),
            Ok(_) => {}
            Err(why) => return Row::not_checked(NAME, super::one_line(&why, 1024)),
        }
    }
    if stale.is_empty() {
        let names: Vec<String> = on.iter().map(|a| a.harness().to_owned()).collect();
        return Row::ok(
            NAME,
            format!("current with this charter for {}", named(&names)),
        );
    }
    Row::warn(
        NAME,
        format!(
            "installed for {}, but not what this charter would install now",
            named(&stale)
        ),
        format!(
            "An older copy lacks this charter's newer hooks and skills. {REPAIR} brings it up \
             to date; `charter plugin install --dry-run` says what differs."
        ),
    )
}

pub(super) fn plugin_files(d: &Doctor) -> Row {
    const NAME: &str = "plugin files";
    let m = match machine(d, NAME) {
        Ok(m) => m,
        Err(row) => return row,
    };
    let on = match installed(m, NAME) {
        Ok(on) => on,
        Err(row) => return row,
    };
    if on.is_empty() {
        return Row::ok(NAME, "nothing installed to run");
    }
    let mut gone: Vec<(String, PathBuf)> = Vec::new();
    let mut runs: Vec<PathBuf> = Vec::new();
    for a in &on {
        match a.runs(m) {
            Some(binary) if binary.is_file() => {
                if !runs.contains(&binary) {
                    runs.push(binary);
                }
            }
            Some(binary) => gone.push((a.harness().to_owned(), binary)),
            None => gone.push((a.harness().to_owned(), PathBuf::new())),
        }
    }
    if gone.is_empty() {
        let shown: Vec<String> = runs.iter().map(|p| fsx::path_field(p)).collect();
        return Row::ok(NAME, format!("the hooks run {}", named(&shown)));
    }
    let said: Vec<String> = gone
        .iter()
        .map(|(harness, binary)| {
            if binary.as_os_str().is_empty() {
                format!("{harness}'s hooks name no charter charter can read")
            } else {
                format!(
                    "{harness}'s hooks run {}, which is not there",
                    fsx::path_field(binary)
                )
            }
        })
        .collect();
    Row::warn(
        NAME,
        said.join("; "),
        format!(
            "A hook whose program is missing cannot start, and the harness then lets the tool \
             call through: the guard does not run in a chat started outside the app. {REPAIR} \
             points the hooks at this charter."
        ),
    )
}

pub(super) fn superseded_plugin(d: &Doctor) -> Row {
    const NAME: &str = "superseded plugin";
    let mut files: Vec<PathBuf> = Vec::new();
    if d.has_plane {
        files.extend(install::superseded_in_plane(&d.root));
    }
    if let Some(m) = &d.machine {
        for a in install::adapters() {
            files.extend(a.superseded(m));
        }
    }
    if files.is_empty() {
        return Row::ok(
            NAME,
            format!(
                "no settings file charter reads enables {}",
                crate::plugin::SUPERSEDED
            ),
        );
    }
    let shown: Vec<String> = files.iter().map(|p| fsx::path_field(p)).collect();
    Row::warn(
        NAME,
        format!(
            "{}, the retired Python charter's plugin, is enabled in {}",
            crate::plugin::SUPERSEDED,
            named(&shown)
        ),
        format!(
            "Delete its `{}` entry from each file named — `charter plugin install` turns it off \
             in your user settings and Codex's config, and a plane's own file is yours to edit. \
             Chats outside the app get charter's own plugin from `charter plugin install`; the \
             app's chats turn the old one off themselves.",
            crate::plugin::SUPERSEDED
        ),
    )
}
