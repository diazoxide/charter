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
//!
//! **And each reader says it left the file out** (charter-app#319), in the ignore check's own
//! sentence — the one the Project settings tab's Local section says — so every group of a
//! settings tab that shows what is in force can say why a Local value is not.

use std::fs;
use std::path::Path;

use charter_core::extension::project::{self, theme};
use charter_core::harness_plugin;
use charter_core::profiles::LOCAL_FILE;
use charter_core::settings::{self, LayerText, Which};

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

/// The ignore check's sentence for the plane at `root`.
fn why(root: &Path) -> String {
    charter_core::profiles::ignore_check(root).reason
}

/// The two layers of the plane at `root` as `layer_text` hands them there: Shared read, and
/// Local left out with its sentence.
fn left_out(root: &Path) -> (LayerText, LayerText) {
    (
        LayerText::Text(SHARED.to_owned()),
        LayerText::LeftOut {
            why: why(root),
            text: LOCAL.to_owned(),
        },
    )
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
    assert_eq!(settings::layer_text(root, Which::Local).text(), Some(LOCAL));
    assert_eq!(settings::layer_text(root, Which::Local).left_out(), None);
}

#[test]
fn a_local_file_git_would_commit_or_tracks_decides_no_extension() {
    charter_core::unsteered!();
    for how in [Git::Committable, Git::Tracked] {
        let dir = plane(how);
        let root = dir.path();
        assert!(refused(root), "{how:?}");

        let (shared, local) = left_out(root);
        let without = || project::Choices::from_layers(&shared, &local);
        assert_eq!(project::Choices::read(root), without(), "{how:?}");
        assert_eq!(
            project::Choices::read_in(root, Some("alpha")),
            without().in_workspace("alpha", Some(MANIFEST)),
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

        let (shared, local) = left_out(root);
        let without = || theme::Said::from_layers(&shared, &local);
        assert_eq!(theme::Said::read(root), without(), "{how:?}");
        assert_eq!(
            theme::Said::read_in(root, Some("alpha")),
            without().in_workspace("alpha", Some(MANIFEST)),
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

        let (shared, local) = left_out(root);
        let without = || harness_plugin::Choices::from_layers(&shared, &local);
        assert_eq!(harness_plugin::Choices::read(root), without(), "{how:?}");
        assert_eq!(
            harness_plugin::Choices::read_in(root, Some("alpha")),
            without().in_workspace("alpha", Some(MANIFEST)),
            "{how:?}"
        );
    }
}

#[test]
fn the_shared_file_is_read_whatever_git_says_about_the_local_one() {
    charter_core::unsteered!();
    let dir = plane(Git::Tracked);

    assert_eq!(
        settings::layer_text(dir.path(), Which::Shared).text(),
        Some(SHARED)
    );
    assert_eq!(settings::layer_text(dir.path(), Which::Local).text(), None);
}

#[test]
fn each_reader_says_it_left_the_local_file_out_in_the_local_sections_words() {
    charter_core::unsteered!();
    for how in [Git::Committable, Git::Tracked] {
        let dir = plane(how);
        let root = dir.path();
        let local = settings::read(root, Which::Local).unwrap();
        let said = local
            .refusals
            .iter()
            .find(|one| one.contains("charter reads nothing in it"))
            .unwrap_or_else(|| panic!("{how:?}: {:?}", local.refusals))
            .as_str();

        assert_eq!(
            settings::layer_text(root, Which::Local).left_out(),
            Some(said),
            "{how:?}"
        );
        for workspace in [None, Some("alpha")] {
            assert_eq!(
                project::Choices::read_in(root, workspace).local_left_out(),
                Some(said),
                "{how:?} {workspace:?}: extensions"
            );
            assert_eq!(
                theme::Said::read_in(root, workspace).local_left_out(),
                Some(said),
                "{how:?} {workspace:?}: theme"
            );
            assert_eq!(
                harness_plugin::Choices::read_in(root, workspace).local_left_out("claude"),
                Some(said),
                "{how:?} {workspace:?}: harness plugins"
            );
        }
    }
}

#[test]
fn a_local_file_that_is_read_or_is_not_there_is_left_out_of_nothing() {
    charter_core::unsteered!();
    let ignored = plane(Git::Ignored);
    let absent = plane(Git::Committable);
    fs::remove_file(absent.path().join(LOCAL_FILE)).unwrap();

    for root in [ignored.path(), absent.path()] {
        assert_eq!(settings::layer_text(root, Which::Local).left_out(), None);
        assert_eq!(settings::layer_text(root, Which::Shared).left_out(), None);
        for workspace in [None, Some("alpha")] {
            assert_eq!(
                project::Choices::read_in(root, workspace).local_left_out(),
                None
            );
            assert_eq!(theme::Said::read_in(root, workspace).local_left_out(), None);
            assert_eq!(
                harness_plugin::Choices::read_in(root, workspace).local_left_out("claude"),
                None
            );
        }
    }
}

#[test]
fn a_left_out_local_file_is_said_only_where_it_would_have_decided_something() {
    charter_core::unsteered!();
    // A Local file that sets only a default profile, and one plugin of one harness: nothing it
    // says was an extension or a theme, so those groups have nothing that was not applied.
    let dir = plane(Git::Committable);
    let root = dir.path();
    fs::write(
        root.join(LOCAL_FILE),
        "[harness]\ndefault = \"claude\"\n\n[harness_plugins.claude]\n\"figma@official\" = false\n",
    )
    .unwrap();
    let said = why(root);

    for workspace in [None, Some("alpha")] {
        assert_eq!(
            project::Choices::read_in(root, workspace).local_left_out(),
            None
        );
        assert_eq!(theme::Said::read_in(root, workspace).local_left_out(), None);
        let plugins = harness_plugin::Choices::read_in(root, workspace);
        assert_eq!(plugins.local_left_out("claude"), Some(said.as_str()));
        assert_eq!(plugins.local_left_out("codex"), None);
        assert_eq!(plugins.local_left_out("opencode"), None);
    }
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
