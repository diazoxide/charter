//! LIVE and LOCAL from the window (charter-app#301, ADR 0051): what making a workspace LIVE
//! would publish and where, and the switch itself — which saves the plane at once, because
//! going LIVE is an explicit intent to publish and going LOCAL has to stop tracking now.
//!
//! The switch is the core's `charter workspace live [--off]` (`wscmd::live`): the `.gitignore`
//! managed block, and `git rm --cached` of what the block published when going LOCAL. Nothing
//! here decides what a workspace publishes; it asks the core the paths the block names.

use std::path::Path;

use purlis_core::planegit::Trigger;
use purlis_core::repocmd::Say;
use purlis_core::wscmd;

use crate::planes::{PlaneId, Planes};

/// What switching a workspace would do, for the confirmation that asks first.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LivePreview {
    /// Whether it is LIVE now.
    pub live: bool,
    /// The workspace's files the block publishes (or stops publishing), plane-relative.
    pub files: Vec<String>,
    /// Where the plane is pushed: `origin` as git has it, or `null` when the plane has none.
    /// Any remote, not only a forge charter knows — a save pushes to it all the same.
    pub remote: Option<String>,
    /// `[plane] mode`, so the confirmation can say whether a save will push at all.
    pub mode: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn workspace_live_preview(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    name: String,
) -> Result<LivePreview, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || preview(&root, &name))
        .await
        .map_err(|err| format!("reading the workspace did not finish: {err}"))?
}

/// Make the workspace `name` LIVE (`live`) or LOCAL, then save the plane. Answers every line
/// both said, or the refusal.
#[tauri::command]
#[specta::specta]
pub async fn workspace_live(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    name: String,
    live: bool,
) -> Result<LiveSwitched, String> {
    let held = planes.held(&plane)?;
    let root = held.root().to_path_buf();
    let switched = tauri::async_runtime::spawn_blocking(move || switch(&root, &name, live))
        .await
        .map_err(|err| format!("switching the workspace did not finish: {err}"))?;
    // The sidebar marks it, and is read straight after (FD-10b).
    held.workspaces_moved();
    switched
}

/// **Who can read what the project's saves push**, for the going-LIVE confirmation (ADR 0051,
/// #1369): its `origin`'s visibility, as the forge answers it.
///
/// Asked apart from [`workspace_live_preview`] because it asks the network: the confirmation
/// shows what it publishes and where at once, and says who reads it when the forge answers.
#[tauri::command]
#[specta::specta]
pub async fn plane_remote_readers(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<RemoteReaders, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        readers_of(
            &root,
            &purlis_core::forge::Caller::window(),
            std::sync::Arc::new(purlis_core::forge::cli::Cli::default()),
        )
    })
    .await
    .map_err(|err| format!("asking who can read the remote did not finish: {err}"))
}

/// Who can read what a push to the project's `origin` publishes.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RemoteReaders {
    /// No one else: the project has no `origin`, or it is a path on this machine.
    Nobody,
    /// Everyone.
    Public,
    /// Everyone signed in to `host`: GitLab's `internal`, GitHub Enterprise's.
    Internal { host: String },
    /// Only those given access to the repository.
    Private,
    /// Not known, and why on one line: a host no forge purlis knows, or a forge that did not
    /// answer. Never read as private.
    Unknown { why: String },
}

/// [`plane_remote_readers`], without a runtime, asking as `caller` over `transport`.
pub fn readers_of(
    root: &Path,
    caller: &purlis_core::forge::Caller,
    transport: std::sync::Arc<dyn purlis_core::forge::transport::Transport>,
) -> RemoteReaders {
    use purlis_core::doctor::Readers;
    use purlis_core::forge::backend::Visibility;
    let Some(url) = origin_of(root) else {
        return RemoteReaders::Nobody;
    };
    match purlis_core::doctor::readers(&url, root, caller, transport) {
        Readers::ThisMachine => RemoteReaders::Nobody,
        Readers::Known { visibility, host } => match visibility {
            Visibility::Public => RemoteReaders::Public,
            Visibility::Internal => RemoteReaders::Internal { host },
            Visibility::Private => RemoteReaders::Private,
        },
        Readers::Unknown(why) => RemoteReaders::Unknown { why },
    }
}

/// [`workspace_live_preview`], without a runtime.
pub fn preview(root: &Path, name: &str) -> Result<LivePreview, String> {
    if !purlis_core::contain::workspace_name_ok(name) {
        return Err(format!("invalid workspace name '{name}'"));
    }
    Ok(LivePreview {
        live: wscmd::live_workspaces(root).contains(name),
        files: wscmd::meta_paths(root, name),
        remote: origin_of(root),
        mode: purlis_core::planesave::Settings::read(root)
            .plane
            .mode
            .value
            .map(|mode| mode.as_str().to_owned()),
    })
}

/// What a switch did: every line it and the save said, and — kept apart, because the switch
/// has already happened — why the save did not.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LiveSwitched {
    pub said: Vec<String>,
    /// Why the plane was not saved after the switch: refused, or not asked yet how it saves.
    pub not_saved: Option<String>,
}

/// `origin` as git has it, whichever host it is on.
fn origin_of(root: &Path) -> Option<String> {
    purlis_core::worktree::git::run(
        root,
        &["remote", "get-url", "origin"],
        purlis_core::worktree::git::READ,
    )
    .ok()
    .filter(purlis_core::worktree::git::Run::ok)
    .map(|r| r.line().trim().to_owned())
    .filter(|url| !url.is_empty())
}

/// The sentence for a plane that has not been asked how it saves.
pub const NOT_ASKED: &str = "Not saved: this project has not been told how it is saved yet — \
                             choose in the Saving tab, and the next save takes the switch.";

/// [`workspace_live`], without a runtime.
pub fn switch(root: &Path, name: &str, live: bool) -> Result<LiveSwitched, String> {
    let mut said: Vec<Say> = Vec::new();
    let code = wscmd::live::live(root, name, !live, &mut |line: Say| said.push(line));
    let mut lines = crate::workspaces::ran(code, said)?;
    let not_saved = save_after(root, &mut lines);
    Ok(LiveSwitched {
        said: lines,
        not_saved,
    })
}

/// Save the plane after a switch — at once, whatever auto-save is set to, because this is the
/// operator publishing or withdrawing on purpose — unless the plane has not been asked how it
/// saves: going LIVE is not an answer to that question, and a save would push every other
/// change in the plane with it (ADR 0051). Answers why it did not save, if it did not.
pub fn save_after(root: &Path, lines: &mut Vec<String>) -> Option<String> {
    if purlis_core::planesave::Settings::read(root)
        .plane
        .mode
        .value
        .is_none()
    {
        return Some(NOT_ASKED.to_owned());
    }
    match crate::saving::save_as(root, None, Trigger::Live) {
        Ok(saved) => {
            lines.extend(saved);
            None
        }
        Err(refused) => Some(refused),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purlis_core::planegit;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> String {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(dir)
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid");
        let out = purlis_core::forklock::output(&mut command).expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// A plane with a workspace `alpha`, committed, in `mode`, and no remote.
    fn plane(mode: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("charter.toml"),
            format!("[plane]\nmode = \"{mode}\"\n"),
        )
        .unwrap();
        for d in ["personas", "inventory", "workspaces"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        std::fs::write(
            root.join(".gitignore"),
            ".charter/\n/charter.local.toml\n/workspaces/*/*\n!/workspaces/.gitkeep\n",
        )
        .unwrap();
        std::fs::write(root.join("workspaces/.gitkeep"), "").unwrap();
        git(root, &["init", "-q", "-b", "main"]);
        git(root, &["config", "user.name", "t"]);
        git(root, &["config", "user.email", "t@example.invalid"]);
        crate::workspaces::create_for_tests(root, "alpha");
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "one"]);
        dir
    }

    #[test]
    fn the_confirmation_is_told_what_a_workspace_publishes_and_that_it_is_local_now() {
        let dir = plane("commit");
        let got = preview(dir.path(), "alpha").expect("read");
        assert!(!got.live);
        assert!(
            got.files
                .contains(&"workspaces/alpha/workspace.md".to_owned()),
            "{:?}",
            got.files
        );
        assert_eq!((got.remote, got.mode.as_deref()), (None, Some("commit")));
    }

    #[test]
    fn going_live_publishes_the_workspace_and_saves_the_plane_at_once() {
        let dir = plane("commit");

        let got = switch(dir.path(), "alpha", true).expect("switched");
        let said = &got.said;

        assert_eq!(got.not_saved, None);
        assert!(said.iter().any(|l| l.contains("is now LIVE")), "{said:?}");
        assert!(preview(dir.path(), "alpha").unwrap().live);
        let tracked = git(dir.path(), &["ls-files", "workspaces/alpha"]);
        assert!(
            tracked.contains("workspaces/alpha/workspace.md"),
            "{tracked}"
        );
        let last = &planegit::journal(dir.path())[0];
        assert_eq!(
            (last["trigger"].as_str(), last["outcome"].as_str()),
            (Some("live"), Some("committed"))
        );
    }

    #[test]
    fn going_local_stops_tracking_the_files_keeps_them_on_disk_and_saves() {
        let dir = plane("commit");
        switch(dir.path(), "alpha", true).expect("live");

        let got = switch(dir.path(), "alpha", false).expect("local");
        let said = &got.said;

        assert!(said.iter().any(|l| l.contains("is now LOCAL")), "{said:?}");
        assert!(!preview(dir.path(), "alpha").unwrap().live);
        assert_eq!(git(dir.path(), &["ls-files", "workspaces/alpha"]), "");
        assert!(
            dir.path().join("workspaces/alpha/workspace.md").is_file(),
            "kept on disk"
        );
        assert_eq!(
            git(dir.path(), &["status", "--porcelain"]),
            "",
            "and the untracking saved: {said:?}"
        );
    }

    #[test]
    fn a_workspace_that_is_not_there_is_refused_in_the_cores_words() {
        let dir = plane("commit");
        let err = switch(dir.path(), "nope", true).expect_err("refused");
        assert!(err.contains("no workspace 'nope'"), "{err}");
    }

    #[test]
    fn a_plane_not_yet_asked_how_it_saves_is_switched_and_not_saved() {
        // Going LIVE is not the answer to how a plane saves, and a save would carry every other
        // change in the plane with it.
        let dir = plane("commit");
        std::fs::write(
            dir.path().join("charter.toml"),
            "[memory]\nshare = \"local\"\n",
        )
        .unwrap();
        git(dir.path(), &["commit", "-qam", "not asked"]);

        let got = switch(dir.path(), "alpha", true).expect("switched");

        assert_eq!(got.not_saved.as_deref(), Some(NOT_ASKED));
        assert!(preview(dir.path(), "alpha").unwrap().live);
        assert!(
            planegit::journal(dir.path()).is_empty(),
            "nothing was saved"
        );
    }

    /// A forge that answers `path` with `out`, or, with `None`, answers nothing.
    fn forge(
        answer: Option<(&str, serde_json::Value)>,
    ) -> std::sync::Arc<dyn purlis_core::forge::transport::Transport> {
        let exchanges = match answer {
            Some((path, out)) => serde_json::json!([{
                "call": {"endpoint": {"rest": {"method": null, "path": path}}, "fields": []},
                "reply": {"code": 0, "out": out.to_string()}
            }]),
            None => serde_json::json!([]),
        };
        let text = serde_json::json!({"source": "test", "exchanges": exchanges}).to_string();
        std::sync::Arc::new(purlis_core::forge::recorded::Recorded::parse(&text).unwrap())
    }

    #[test]
    fn the_confirmation_is_told_a_public_origin_is_public() {
        let dir = plane("push");
        git(
            dir.path(),
            &["remote", "add", "origin", "git@github.com:acme/plane.git"],
        );
        let got = readers_of(
            dir.path(),
            &purlis_core::forge::Caller::window(),
            forge(Some((
                "repos/acme/plane",
                serde_json::json!({"visibility": "public"}),
            ))),
        );
        assert_eq!(got, RemoteReaders::Public);
    }

    #[test]
    fn a_project_with_no_origin_publishes_to_nobody_and_an_unanswered_forge_is_unknown() {
        let dir = plane("push");
        let window = purlis_core::forge::Caller::window();
        assert_eq!(
            readers_of(dir.path(), &window, forge(None)),
            RemoteReaders::Nobody
        );
        git(
            dir.path(),
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/acme/plane.git",
            ],
        );
        assert!(
            matches!(
                readers_of(dir.path(), &window, forge(None)),
                RemoteReaders::Unknown { .. }
            ),
            "an unanswered ask is never read as private"
        );
    }

    #[test]
    fn any_origin_is_named_not_only_a_forge_charter_knows() {
        let dir = plane("push");
        git(
            dir.path(),
            &["remote", "add", "origin", "git@git.corp:team/plane.git"],
        );
        assert_eq!(
            preview(dir.path(), "alpha").unwrap().remote.as_deref(),
            Some("git@git.corp:team/plane.git")
        );
    }
}
