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

use std::path::Path;
use std::sync::Arc;

use super::git::git_in;
use super::{Doctor, Row, first_line};
use crate::forge::backend::{About, PushProtection, RepoRecord, Visibility};
use crate::forge::transport::Transport;
use crate::forge::{self, Caller, Forge, Kind};

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
    match ask(&url, root, d.forges.as_ref().map(|(c, t)| (c, t.clone()))) {
        Asked::ThisMachine => Row::ok(
            NAME,
            format!(
                "origin is on this machine ({}), so a push publishes nothing",
                super::one_line(&url, super::PATH_DISPLAY_LIMIT)
            ),
        ),
        Asked::Unknown(why) => Row::not_checked(NAME, why),
        Asked::Answered { on, path, about } => {
            said(&on, &path, about.visibility, about.push_protection)
        }
    }
}

/// What asking the forge about `url` came to.
enum Asked {
    /// `url` is a path on this machine: nothing is asked, and a push publishes nothing.
    ThisMachine,
    /// Nothing that says who can read it, and why, on one line.
    Unknown(String),
    /// The forge `on` answered for the repository at `path`.
    Answered {
        on: Forge,
        path: String,
        about: About,
    },
}

/// Ask the forge `url` is on about it, as `caller` over `transport` — or, with no forge to ask,
/// say so. One reading for the doctor's row and the going-LIVE confirmation, so the two never
/// disagree about one remote.
fn ask(url: &str, root: &Path, forges: Option<(&Caller, Arc<dyn Transport>)>) -> Asked {
    let host = forge::host_of(url);
    if host.is_empty() {
        return Asked::ThisMachine;
    }
    let Some(on) = forge::resolve_host(url, root) else {
        return Asked::Unknown(format!(
            "origin is on {}, a host no forge purlis knows names, so who can read it is not known",
            super::one_line(&host, super::DISPLAY_LIMIT)
        ));
    };
    let Some(path) = forge::namespace_of(url) else {
        return Asked::Unknown("origin names no repository purlis can read".to_owned());
    };
    let Some((caller, transport)) = forges else {
        return Asked::Unknown("this doctor asks no forge".to_owned());
    };
    match on.backend_over(transport).about(caller, &repo(&on, &path)) {
        Ok(about) => Asked::Answered { on, path, about },
        // The forge's or its CLI's own words, which can run to several lines.
        Err(e) => Asked::Unknown(super::one_line(e.said(), super::DISPLAY_LIMIT)),
    }
}

/// **Who can read what a push to `url` publishes**: the going-LIVE confirmation's question
/// (ADR 0051, #1369), answered by the same forge read as the doctor's `project remote` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Readers {
    /// `url` is a path on this machine, so a push publishes it to nobody.
    ThisMachine,
    /// The forge on `host` said who can read the repository.
    Known {
        visibility: Visibility,
        host: String,
    },
    /// Not known, and why, on one line: a host no forge names, or a forge that did not answer.
    Unknown(String),
}

/// [`Readers`] for a push to `url` from the project at `root`, asking its forge as `caller`
/// over `transport`. It asks the network: call it off the UI thread, and only where a person
/// is about to publish.
pub fn readers(url: &str, root: &Path, caller: &Caller, transport: Arc<dyn Transport>) -> Readers {
    match ask(url, root, Some((caller, transport))) {
        Asked::ThisMachine => Readers::ThisMachine,
        Asked::Unknown(why) => Readers::Unknown(why),
        Asked::Answered { on, about, .. } => Readers::Known {
            visibility: about.visibility,
            host: on.host,
        },
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A forge that answers `path` with `out`, or, with `None`, answers nothing.
    fn forge(answer: Option<(&str, serde_json::Value)>) -> Arc<dyn Transport> {
        let exchanges = match answer {
            Some((path, out)) => serde_json::json!([{
                "call": {"endpoint": {"rest": {"method": null, "path": path}}, "fields": []},
                "reply": {"code": 0, "out": out.to_string()}
            }]),
            None => serde_json::json!([]),
        };
        let text = serde_json::json!({"source": "test", "exchanges": exchanges}).to_string();
        Arc::new(crate::forge::recorded::Recorded::parse(&text).unwrap())
    }

    fn asked(url: &str, answer: Option<(&str, serde_json::Value)>) -> Readers {
        // No manifest: the two forges' own hosts resolve without one.
        let root = tempfile::tempdir().unwrap();
        readers(url, root.path(), &Caller::window(), forge(answer))
    }

    #[test]
    fn a_public_github_remote_is_said_to_be_public() {
        let got = asked(
            "git@github.com:acme/plane.git",
            Some((
                "repos/acme/plane",
                serde_json::json!({"visibility": "public"}),
            )),
        );
        assert_eq!(
            got,
            Readers::Known {
                visibility: Visibility::Public,
                host: "github.com".to_owned(),
            }
        );
    }

    #[test]
    fn a_gitlab_remote_answers_its_own_visibility() {
        let got = asked(
            "https://gitlab.com/acme/ops/plane.git",
            Some((
                "projects/acme%2Fops%2Fplane",
                serde_json::json!({"visibility": "internal"}),
            )),
        );
        assert_eq!(
            got,
            Readers::Known {
                visibility: Visibility::Internal,
                host: "gitlab.com".to_owned(),
            }
        );
    }

    #[test]
    fn a_remote_on_this_machine_publishes_to_nobody() {
        assert_eq!(asked("/srv/git/plane.git", None), Readers::ThisMachine);
    }

    #[test]
    fn a_forge_that_does_not_answer_leaves_it_unknown_and_says_why_on_one_line() {
        let Readers::Unknown(why) = asked("https://github.com/acme/plane.git", None) else {
            panic!("an unanswered ask is never a visibility");
        };
        assert!(!why.is_empty() && !why.contains('\n'), "{why}");
    }

    #[test]
    fn a_host_no_forge_names_is_unknown_and_named() {
        let Readers::Unknown(why) = asked("https://git.example.invalid/acme/plane.git", None)
        else {
            panic!("a host no forge names is never a visibility");
        };
        assert!(why.contains("git.example.invalid"), "{why}");
    }
}
