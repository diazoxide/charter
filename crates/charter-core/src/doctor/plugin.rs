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
    let detail = format!(
        "not installed for {} — a chat started outside the app runs without charter's hooks \
         and guard",
        named(&missing)
    );
    // The preflight runs inside a chat, which the app or the plugin has already armed, and a
    // warning there would be counted in every app chat's status line for a choice about OTHER
    // chats. The doctor a person types says it as a warning.
    if d.preflight {
        return Row::ok(NAME, detail);
    }
    Row::warn(
        NAME,
        detail,
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
    // Asked as if the charter the hooks already run had installed it: which binary they run is
    // `plugin files`' question, and a doctor run by another charter — a development build, one
    // on PATH — must not call a copy stale for naming the app's.
    let (mut stale, mut current, mut unknown) = (Vec::new(), Vec::new(), Vec::new());
    for a in &on {
        let mut as_installed = m.clone();
        if let Some(binary) = a.runs(m) {
            as_installed.binary = binary;
        }
        match a.install(&as_installed) {
            Ok(plan) if plan.changes() => stale.push(a.harness().to_owned()),
            Ok(_) => current.push(a.harness().to_owned()),
            Err(why) => unknown.push(format!(
                "{} not compared ({})",
                a.harness(),
                super::one_line(&why, 1024)
            )),
        }
    }
    let tail: String = unknown.iter().map(|why| format!("; {why}")).collect();
    if stale.is_empty() {
        if current.is_empty() {
            return Row::not_checked(NAME, unknown.join("; "));
        }
        return Row::ok(
            NAME,
            format!("current with this charter for {}{tail}", named(&current)),
        );
    }
    Row::warn(
        NAME,
        format!(
            "installed for {}, but not what this charter would install now{tail}",
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
    let mut said: Vec<String> = Vec::new();
    let mut runs: Vec<PathBuf> = Vec::new();
    for a in &on {
        match a.runs(m) {
            Some(binary) if binary.is_file() => {
                if !runs.contains(&binary) {
                    runs.push(binary);
                }
            }
            Some(binary) => said.push(format!(
                "{}'s hooks run {}, which is not there",
                a.harness(),
                fsx::path_field(&binary)
            )),
            // The setting is there and the copy's hooks file is missing or unreadable.
            None => said.push(format!(
                "{} is enabled, but its hooks could not be read to see which charter they run",
                a.harness()
            )),
        }
    }
    if said.is_empty() {
        let shown: Vec<String> = runs.iter().map(|p| fsx::path_field(p)).collect();
        return Row::ok(NAME, format!("the hooks run {}", named(&shown)));
    }
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
    let mut layers = false;
    if d.has_plane {
        files.extend(install::superseded_in_plane(&d.root));
        // A workspace's generated settings, written before charter stopped carrying the
        // retired plugin into them: `charter workspace reinit` rewrites them without it.
        if let Ok((names, _)) = fsx::read_workspaces(&d.root) {
            for ws in names {
                let layer = d
                    .root
                    .join("workspaces")
                    .join(ws)
                    .join(".claude/settings.json");
                if install::enables_superseded(&layer) {
                    files.push(layer);
                    layers = true;
                }
            }
        }
    }
    match &d.machine {
        Some(m) => {
            for a in install::adapters() {
                files.extend(a.superseded(m));
            }
        }
        // It did not look at this machine's own files, so it cannot say none enables it.
        None if files.is_empty() => {
            return Row::not_checked(
                NAME,
                "the plane's files do not enable it, and charter cannot tell where Claude Code \
                 and Codex keep this machine's configuration",
            );
        }
        None => {}
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
             in your user settings and Codex's config, and a plane's own file is yours to \
             edit.{} Chats outside the app get charter's own plugin from `charter plugin \
             install`; the app's chats turn the old one off themselves.",
            crate::plugin::SUPERSEDED,
            if layers {
                " A workspace's generated settings are rewritten without it by `charter \
                 workspace reinit --all`."
            } else {
                ""
            }
        ),
    )
}
