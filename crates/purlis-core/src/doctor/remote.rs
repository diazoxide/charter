//! `project remote`: who can read what the project's saves push, and whether its forge refuses a
//! push that carries a secret (SQ-8, gap G4d).
//!
//! charter's own scans run where charter is: `charter save` scans what it stages, and a chat's
//! git scans its commits and pushes (ADR 0074). A push from anywhere else reaches the forge
//! unscanned, and on a public remote that is a publication. The forge's own guard covers every
//! push: GitHub's push protection, GitLab's secret push protection. This row names the remote's
//! visibility and that setting, asked through the forge seam's [`Repos::about`], which both
//! forges answer from their own fields.
//!
//! **Only a doctor someone asked for asks the forge.** The preflight runs at every session start
//! and asks no network, so it has no row at all, rather than a green one over nothing asked.
//!
//! [`Repos::about`]: crate::forge::backend::Repos::about

use super::git::git_in;
use super::{Doctor, Row, first_line};
use crate::forge::backend::{PushProtection, RepoRecord, Visibility};
use crate::forge::{self, Forge, Kind};

const NAME: &str = "project remote";

pub(super) fn project_remote(d: &Doctor) -> Option<Row> {
    if d.preflight {
        return None;
    }
    Some(row(d))
}

fn row(d: &Doctor) -> Row {
    if !d.has_plane {
        return Row::ok(NAME, "no control plane found");
    }
    let root = d.root.as_path();
    let top = match git_in(root, &["rev-parse", "--show-toplevel"]) {
        Ok(top) => top,
        Err(why) => return Row::not_checked(NAME, why),
    };
    let toplevel = first_line(&top.out);
    if !top.ok() || toplevel.is_empty() {
        return Row::ok(NAME, "not a git repository");
    }
    // Another repository's remote is not the project's, as `plane root` reads it.
    if super::canonical(std::path::Path::new(&toplevel)) != super::canonical(root) {
        return Row::ok(
            NAME,
            format!(
                "not its own repository (inside {})",
                super::one_line(&toplevel, super::PATH_DISPLAY_LIMIT)
            ),
        );
    }
    let url = match git_in(root, &["remote", "get-url", "origin"]) {
        Ok(run) if run.ok() => first_line(&run.out),
        Ok(_) => return Row::ok(NAME, "no origin remote, so nothing is pushed"),
        Err(why) => return Row::not_checked(NAME, why),
    };
    let host = forge::host_of(&url);
    if host.is_empty() {
        return Row::ok(
            NAME,
            format!(
                "origin is on this machine ({}), so a push publishes nothing",
                super::one_line(&url, super::PATH_DISPLAY_LIMIT)
            ),
        );
    }
    let Some(on) = forge::resolve_host(&url, root) else {
        return Row::not_checked(
            NAME,
            format!(
                "origin is on {}, a host no forge purlis knows names, so who can read it is \
                 not known",
                super::one_line(&host, super::DISPLAY_LIMIT)
            ),
        );
    };
    let Some(path) = forge::namespace_of(&url) else {
        return Row::not_checked(NAME, "origin names no repository purlis can read");
    };
    let Some((caller, transport)) = &d.forges else {
        return Row::not_checked(NAME, "this doctor asks no forge");
    };
    let about = match on
        .backend_over(transport.clone())
        .about(caller, &repo(&on, &path))
    {
        Ok(about) => about,
        // The forge's or its CLI's own words, which can run to several lines.
        Err(e) => {
            return Row::not_checked(NAME, super::one_line(e.said(), super::DISPLAY_LIMIT));
        }
    };
    said(&on, &path, about.visibility, about.push_protection)
}

/// The repo at `path` on `on`, as [`crate::forge::backend::Repos::about`] addresses it: by its
/// path alone.
fn repo(on: &Forge, path: &str) -> RepoRecord {
    RepoRecord {
        id: None,
        name: path.rsplit('/').next().unwrap_or(path).to_owned(),
        path_with_namespace: path.to_owned(),
        default_branch: None,
        description: String::new(),
        web_url: String::new(),
        ssh_url: String::new(),
        topics: Vec::new(),
        forge: on.kind,
    }
}

/// The row for what the forge answered.
fn said(on: &Forge, path: &str, visibility: Visibility, protection: PushProtection) -> Row {
    let kind = on.kind;
    let noun = kind.push_protection_noun();
    let readers = match visibility {
        Visibility::Public => "public — everyone can read what is pushed".to_owned(),
        Visibility::Internal => format!(
            "internal — everyone signed in to {} can read what is pushed",
            on.host
        ),
        Visibility::Private => "private".to_owned(),
    };
    let state = match protection {
        PushProtection::On => format!("{noun} is on"),
        PushProtection::Off => format!("{noun} is off"),
        PushProtection::Unknown => format!("whether {noun} is on is not shown to this account"),
    };
    let detail = format!(
        "{} on {} is {readers}; {state}",
        super::one_line(path, super::DISPLAY_LIMIT),
        kind.display()
    );
    if visibility == Visibility::Private || protection == PushProtection::On {
        return Row::ok(NAME, detail);
    }
    Row::warn(NAME, detail, hint(kind, protection))
}

fn hint(kind: Kind, protection: PushProtection) -> String {
    let noun = kind.push_protection_noun();
    let forge = kind.display();
    // Not every GitLab instance, or every group on GitLab.com, offers it; there the row stays a
    // warning, and saying why is what keeps it from reading as a fault in charter.
    let offered = match kind {
        Kind::GitLab => {
            " Where the instance does not offer it, this row stays a warning until the repository \
             is private."
        }
        Kind::GitHub => "",
    };
    let why = format!(
        "purlis scans what its own saves and chats push; {noun} also refuses a secret in a push \
         from anywhere else.{offered}"
    );
    match protection {
        PushProtection::Unknown => format!(
            "{forge} shows {noun} only to an account that may change it. Check the repository's \
             security settings on {forge}, and turn it on if it is off. {why}"
        ),
        PushProtection::Off | PushProtection::On => format!(
            "Turn on {noun} in the repository's security settings on {forge}, or make the \
             repository private. {why}"
        ),
    }
}
