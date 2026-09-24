//! `charter.local.toml` is this machine's say only while git leaves it alone (charter-app#308).
//!
//! The rule is the local file's own: an ignored file must not change plane policy with no trace
//! in git. A file git tracks, or would commit, is not that file any more — it reaches every clone
//! — so NOTHING in it is read: not its profiles (ADR 0022, already), and not its extensions, its
//! theme or its harness plugins either (ADR 0048, ADR 0050). Every reader of the Local layer takes
//! it through one function, `settings::layer_text`, so a reader added later cannot forget.
//!
//! Each reader is asked of the same three planes: the local file ignored (Local decides), not
//! ignored (Local is not read), and tracked (Local is not read). And in a workspace, whose layer
//! sits between the two files: with Local refused, the workspace has the last word.

use std::fs;
use std::path::Path;

use charter_core::extension::project::{self, theme};
use charter_core::harness_plugin;
use charter_core::profiles::LOCAL_FILE;
use charter_core::settings::{self, Which};

mod support;

const SHARED: &str = "[extensions.stats]\nenabled = true\n\n[theme]\nuse = \"charter-light\"\n\n\
                      [harness_plugins.claude]\n\"figma@official\" = true\n";

const LOCAL: &str = "[extensions.stats]\nenabled = false\n\n[theme]\nuse = \"charter-dark\"\n\n\
                     [harness_plugins.claude]\n\"figma@official\" = false\n";

const MANIFEST: &str = r#"{"settings": {"extensions": {"stats": {"enabled": true}}, "theme": {"use": "system"}, "harness_plugins": {"claude": {"figma@official": true}}}}"#;

/// How git stands to the local file.
#[derive(Clone, Copy, Debug)]
enum Git {
    Ignored,
    Committable,
    Tracked,
}

/// A plane that is a git repository, with both files, a workspace `alpha`, and the local file
/// standing to git as `how` says.
fn plane(how: Git) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("charter.toml"), SHARED).unwrap();
    fs::write(root.join(LOCAL_FILE), LOCAL).unwrap();
    fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
    fs::write(root.join("workspaces/alpha/workspace.json"), MANIFEST).unwrap();
    support::git(root, &["init", "-q"]);
    match how {
        Git::Ignored => fs::write(root.join(".gitignore"), "/charter.local.toml\n").unwrap(),
        Git::Committable => {}
        Git::Tracked => {
            fs::write(root.join(".gitignore"), "/charter.local.toml\n").unwrap();
            support::git(root, &["add", "-f", LOCAL_FILE]);
        }
    }
    dir
}

fn refused(root: &Path) -> bool {
    !charter_core::profiles::ignore_check(root).passes()
}

#[test]
fn an_ignored_local_file_is_read_by_every_reader() {
    charter_core::unsteered!();
    let dir = plane(Git::Ignored);
    let root = dir.path();
    assert!(!refused(root));

    assert_eq!(
        project::Choices::read(root),
        project::Choices::from_text(Some(SHARED), Some(LOCAL))
    );
    assert_eq!(
        theme::Said::read(root),
        theme::Said::from_text(Some(SHARED), Some(LOCAL))
    );
    assert_eq!(
        harness_plugin::Choices::read(root),
        harness_plugin::Choices::from_text(Some(SHARED), Some(LOCAL))
    );
    assert_eq!(
        settings::layer_text(root, Which::Local).as_deref(),
        Some(LOCAL)
    );
}

#[test]
fn a_local_file_git_would_commit_or_tracks_decides_no_extension() {
    charter_core::unsteered!();
    for how in [Git::Committable, Git::Tracked] {
        let dir = plane(how);
        let root = dir.path();
        assert!(refused(root), "{how:?}");

        assert_eq!(
            project::Choices::read(root),
            project::Choices::from_text(Some(SHARED), None),
            "{how:?}"
        );
        assert_eq!(
            project::Choices::read_in(root, Some("alpha")),
            project::Choices::from_text(Some(SHARED), None).in_workspace("alpha", Some(MANIFEST)),
            "{how:?}: in a workspace, the workspace has the last word"
        );
    }
}

#[test]
fn a_local_file_git_would_commit_or_tracks_picks_no_theme() {
    charter_core::unsteered!();
    for how in [Git::Committable, Git::Tracked] {
        let dir = plane(how);
        let root = dir.path();

        assert_eq!(
            theme::Said::read(root),
            theme::Said::from_text(Some(SHARED), None),
            "{how:?}"
        );
        assert_eq!(
            theme::Said::read_in(root, Some("alpha")),
            theme::Said::from_text(Some(SHARED), None).in_workspace("alpha", Some(MANIFEST)),
            "{how:?}"
        );
    }
}

#[test]
fn a_local_file_git_would_commit_or_tracks_chooses_no_harness_plugin() {
    charter_core::unsteered!();
    for how in [Git::Committable, Git::Tracked] {
        let dir = plane(how);
        let root = dir.path();

        assert_eq!(
            harness_plugin::Choices::read(root),
            harness_plugin::Choices::from_text(Some(SHARED), None),
            "{how:?}"
        );
        assert_eq!(
            harness_plugin::Choices::read_in(root, Some("alpha")),
            harness_plugin::Choices::from_text(Some(SHARED), None)
                .in_workspace("alpha", Some(MANIFEST)),
            "{how:?}"
        );
    }
}

#[test]
fn the_shared_file_is_read_whatever_git_says_about_the_local_one() {
    charter_core::unsteered!();
    let dir = plane(Git::Tracked);

    assert_eq!(
        settings::layer_text(dir.path(), Which::Shared).as_deref(),
        Some(SHARED)
    );
    assert_eq!(settings::layer_text(dir.path(), Which::Local), None);
}

#[test]
fn the_project_settings_tab_says_why_the_local_file_is_not_read_and_the_fix() {
    charter_core::unsteered!();
    let dir = plane(Git::Committable);

    let read = settings::read(dir.path(), Which::Local).unwrap();

    assert!(
        read.refusals.contains(
            &"git would commit charter.local.toml, so charter reads nothing in it until it \
              is ignored — charter reinit adds /charter.local.toml to .gitignore."
                .to_owned()
        ),
        "{:?}",
        read.refusals
    );
}

#[test]
fn a_tracked_local_file_is_said_with_its_own_fix() {
    charter_core::unsteered!();
    let dir = plane(Git::Tracked);

    let read = settings::read(dir.path(), Which::Local).unwrap();

    assert!(
        read.refusals.contains(
            &"git tracks charter.local.toml, so what it says would reach every clone of this \
              plane — charter reads nothing in it until it is untracked: git rm --cached \
              charter.local.toml, commit that removal, then charter reinit."
                .to_owned()
        ),
        "{:?}",
        read.refusals
    );
}
