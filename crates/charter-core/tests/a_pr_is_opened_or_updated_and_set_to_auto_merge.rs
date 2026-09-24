//! Opening or updating a pull request (a merge request on GitLab), and asking the forge to
//! merge it once it may: `charter_core::forge::pr`, the forge adapters' first write that is not
//! a push (ADR 0051).
//!
//! Every question goes to a stand-in `gh` or `glab` that answers only what a test wrote down,
//! exactly once (`support/forge_cli.rs`), so nothing here reaches a network and a question
//! nobody expected fails the call that asked it. That is also how "updated, not duplicated" is
//! proved: a test that writes down no create question makes any create exit 99.

mod support;

use std::path::PathBuf;

use charter_core::forge::pr::{self, Pr, Repo};
use support::forge_cli::{Scene, in_a_child, in_child, was_asked};

#[test]
fn every_pull_request_question_is_asked_of_a_stand_in_cli() {
    charter_core::unsteered!();
    in_a_child("prs::", "bin");
}

/// The one GraphQL document that reads what a GitHub repo allows and the PR's node id.
const SETTINGS: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){autoMergeAllowed rebaseMergeAllowed mergeCommitAllowed squashMergeAllowed pullRequest(number:$number){id}}}";

/// The one GraphQL document that turns auto-merge on.
const ENABLE: &str = "mutation($id:ID!,$method:PullRequestMergeMethod!){enablePullRequestAutoMerge(input:{pullRequestId:$id,mergeMethod:$method}){clientMutationId}}";

mod prs {
    use super::*;

    fn repo(scene: &Scene, kind: &str, path: &str) -> Repo {
        Repo {
            forge: scene.forge(kind),
            path: path.to_string(),
        }
    }

    fn github_lookup(scene: &Scene, code: i32, out: &str, err: &str) -> PathBuf {
        scene.gh_api(
            "repos/acme/widget/pulls?state=open&head=acme:charter%2Fsave%2Fmac&base=release&per_page=1",
            code,
            out,
            err,
        )
    }

    fn github_create(scene: &Scene, out: &str) -> PathBuf {
        scene.answers(
            "gh",
            &[
                "api",
                "--hostname",
                &scene.host,
                "-X",
                "POST",
                "repos/acme/widget/pulls",
                "-f",
                "head=charter/save/mac",
                "-f",
                "base=release",
                "-f",
                "title=Save from mac",
                "-f",
                "body=memory: 2 files\n@not-a-file",
            ],
            0,
            out,
            "",
        )
    }

    #[test]
    fn with_no_open_pr_github_opens_one_into_the_base_branch_named() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("pr-open.test");
        let looked = github_lookup(&scene, 0, "[]", "");
        let created = github_create(
            &scene,
            r#"{"number": 12, "html_url": "https://pr-open.test/acme/widget/pull/12"}"#,
        );
        let opened = pr::open_or_update(
            &repo(&scene, "github", "acme/widget"),
            "charter/save/mac",
            "release",
            "Save from mac",
            "memory: 2 files\n@not-a-file",
        );
        assert_eq!(
            opened,
            Ok(Pr {
                number: 12,
                url: "https://pr-open.test/acme/widget/pull/12".into()
            })
        );
        assert!(was_asked(&looked) && was_asked(&created));
    }

    #[test]
    fn an_open_github_pr_for_the_same_head_and_base_is_updated_and_never_duplicated() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("pr-update.test");
        github_lookup(
            &scene,
            0,
            r#"[{"number": 7, "html_url": "https://pr-update.test/acme/widget/pull/7"}]"#,
            "",
        );
        let updated = scene.answers(
            "gh",
            &[
                "api",
                "--hostname",
                "pr-update.test",
                "-X",
                "PATCH",
                "repos/acme/widget/pulls/7",
                "-f",
                "title=Save from mac",
                "-f",
                "body=newer",
            ],
            0,
            r#"{"number": 7, "html_url": "https://pr-update.test/acme/widget/pull/7"}"#,
            "",
        );
        let opened = pr::open_or_update(
            &repo(&scene, "github", "acme/widget"),
            "charter/save/mac",
            "release",
            "Save from mac",
            "newer",
        );
        assert_eq!(
            opened,
            Ok(Pr {
                number: 7,
                url: "https://pr-update.test/acme/widget/pull/7".into()
            })
        );
        assert!(was_asked(&updated));
    }

    #[test]
    fn a_github_lookup_that_fails_opens_nothing_rather_than_a_duplicate() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("pr-lookup-fails.test");
        github_lookup(&scene, 1, "", "HTTP 502: Bad Gateway");
        let created = github_create(&scene, r#"{"number": 1, "html_url": "x"}"#);
        let said = pr::open_or_update(
            &repo(&scene, "github", "acme/widget"),
            "charter/save/mac",
            "release",
            "Save from mac",
            "memory: 2 files\n@not-a-file",
        )
        .unwrap_err();
        assert!(said.contains("HTTP 502: Bad Gateway"), "{said}");
        assert!(!was_asked(&created));
    }

    #[test]
    fn a_github_create_the_forge_refuses_is_reported_in_its_own_words() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("pr-refused.test");
        github_lookup(&scene, 0, "[]", "");
        scene.answers(
            "gh",
            &[
                "api",
                "--hostname",
                "pr-refused.test",
                "-X",
                "POST",
                "repos/acme/widget/pulls",
                "-f",
                "head=charter/save/mac",
                "-f",
                "base=release",
                "-f",
                "title=t",
                "-f",
                "body=b",
            ],
            1,
            r#"{"message":"Validation Failed"}"#,
            "gh: Validation Failed (HTTP 422)",
        );
        let said = pr::open_or_update(
            &repo(&scene, "github", "acme/widget"),
            "charter/save/mac",
            "release",
            "t",
            "b",
        )
        .unwrap_err();
        assert!(said.contains("Validation Failed (HTTP 422)"), "{said}");
    }

    fn gitlab_lookup(scene: &Scene, out: &str) -> PathBuf {
        scene.glab_api(
            "projects/acme%2Fplat%2Fwidget/merge_requests?state=opened&source_branch=charter%2Fsave%2Fmac&target_branch=release&per_page=1",
            0,
            out,
            "",
        )
    }

    #[test]
    fn with_no_open_mr_gitlab_opens_one_into_the_target_branch_named() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("mr-open.test");
        gitlab_lookup(&scene, "[]");
        let created = scene.answers(
            "glab",
            &[
                "--hostname",
                "mr-open.test",
                "api",
                "-X",
                "POST",
                "projects/acme%2Fplat%2Fwidget/merge_requests",
                "-f",
                "source_branch=charter/save/mac",
                "-f",
                "target_branch=release",
                "-f",
                "title=Save from mac",
                "-f",
                "description=body",
            ],
            0,
            r#"{"iid": 3, "id": 9001, "web_url": "https://mr-open.test/acme/plat/widget/-/merge_requests/3"}"#,
            "",
        );
        let opened = pr::open_or_update(
            &repo(&scene, "gitlab", "acme/plat/widget"),
            "charter/save/mac",
            "release",
            "Save from mac",
            "body",
        );
        assert_eq!(
            opened,
            Ok(Pr {
                number: 3,
                url: "https://mr-open.test/acme/plat/widget/-/merge_requests/3".into()
            }),
            "numbered by its iid, the number GitLab shows, not its global id"
        );
        assert!(was_asked(&created));
    }

    #[test]
    fn an_open_gitlab_mr_for_the_same_source_and_target_is_updated_and_never_duplicated() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("mr-update.test");
        gitlab_lookup(
            &scene,
            r#"[{"iid": 4, "web_url": "https://mr-update.test/acme/plat/widget/-/merge_requests/4"}]"#,
        );
        let updated = scene.answers(
            "glab",
            &[
                "--hostname",
                "mr-update.test",
                "api",
                "-X",
                "PUT",
                "projects/acme%2Fplat%2Fwidget/merge_requests/4",
                "-f",
                "title=Save from mac",
                "-f",
                "description=newer",
            ],
            0,
            r#"{"iid": 4, "web_url": "https://mr-update.test/acme/plat/widget/-/merge_requests/4"}"#,
            "",
        );
        let opened = pr::open_or_update(
            &repo(&scene, "gitlab", "acme/plat/widget"),
            "charter/save/mac",
            "release",
            "Save from mac",
            "newer",
        );
        assert_eq!(opened.map(|p| p.number), Ok(4));
        assert!(was_asked(&updated));
    }

    // -- auto-merge ------------------------------------------------------------------------ //

    fn settings(scene: &Scene, allowed: [bool; 4]) -> PathBuf {
        let [auto, rebase, merge, squash] = allowed;
        scene.answers(
            "gh",
            &[
                "api",
                "graphql",
                "--hostname",
                &scene.host,
                "-f",
                &format!("query={SETTINGS}"),
                "-f",
                "owner=acme",
                "-f",
                "name=widget",
                "-F",
                "number=12",
            ],
            0,
            &format!(
                r#"{{"data": {{"repository": {{"autoMergeAllowed": {auto}, "rebaseMergeAllowed": {rebase}, "mergeCommitAllowed": {merge}, "squashMergeAllowed": {squash}, "pullRequest": {{"id": "PR_node12"}}}}}}}}"#
            ),
            "",
        )
    }

    fn enable(scene: &Scene, method: &str, code: i32, out: &str, err: &str) -> PathBuf {
        scene.answers(
            "gh",
            &[
                "api",
                "graphql",
                "--hostname",
                &scene.host,
                "-f",
                &format!("query={ENABLE}"),
                "-f",
                "id=PR_node12",
                "-f",
                &format!("method={method}"),
            ],
            code,
            out,
            err,
        )
    }

    fn twelve(scene: &Scene) -> Pr {
        Pr {
            number: 12,
            url: format!("https://{}/acme/widget/pull/12", scene.host),
        }
    }

    const ENABLED: &str = r#"{"data": {"enablePullRequestAutoMerge": {"clientMutationId": null}}}"#;

    #[test]
    fn github_auto_merge_prefers_rebase_when_the_repo_allows_it() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("am-rebase.test");
        settings(&scene, [true, true, true, true]);
        let asked = enable(&scene, "REBASE", 0, ENABLED, "");
        let repo = repo(&scene, "github", "acme/widget");
        assert_eq!(pr::request_auto_merge(&repo, &twelve(&scene)), Ok(()));
        assert!(was_asked(&asked));
    }

    #[test]
    fn github_auto_merge_takes_a_merge_commit_when_rebase_is_not_allowed() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("am-merge.test");
        settings(&scene, [true, false, true, true]);
        let asked = enable(&scene, "MERGE", 0, ENABLED, "");
        let repo = repo(&scene, "github", "acme/widget");
        assert_eq!(pr::request_auto_merge(&repo, &twelve(&scene)), Ok(()));
        assert!(was_asked(&asked));
    }

    #[test]
    fn github_auto_merge_squashes_only_when_squash_is_all_the_repo_allows() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("am-squash.test");
        settings(&scene, [true, false, false, true]);
        let asked = enable(&scene, "SQUASH", 0, ENABLED, "");
        let repo = repo(&scene, "github", "acme/widget");
        assert_eq!(pr::request_auto_merge(&repo, &twelve(&scene)), Ok(()));
        assert!(was_asked(&asked));
    }

    #[test]
    fn a_github_repo_with_auto_merge_off_is_told_so_and_nothing_is_merged() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("am-off.test");
        settings(&scene, [false, true, true, true]);
        let repo = repo(&scene, "github", "acme/widget");
        let said = pr::request_auto_merge(&repo, &twelve(&scene)).unwrap_err();
        assert!(said.contains("auto-merge"), "{said}");
        assert!(said.contains("acme/widget"), "{said}");
    }

    #[test]
    fn a_github_repo_that_allows_no_merge_method_is_told_so() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("am-none.test");
        settings(&scene, [true, false, false, false]);
        let repo = repo(&scene, "github", "acme/widget");
        assert_eq!(
            pr::request_auto_merge(&repo, &twelve(&scene)),
            Err("acme/widget allows no merge method".into())
        );
    }

    #[test]
    fn a_github_pr_with_nothing_left_to_wait_for_is_merged_by_the_same_method() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("am-clean.test");
        settings(&scene, [true, false, true, true]);
        enable(
            &scene,
            "MERGE",
            1,
            r#"{"data": {"enablePullRequestAutoMerge": null}, "errors": [{"type": "UNPROCESSABLE", "message": "Pull request Pull request is in clean status"}]}"#,
            "gh: Pull request Pull request is in clean status\n",
        );
        let merged = scene.answers(
            "gh",
            &[
                "api",
                "--hostname",
                "am-clean.test",
                "-X",
                "PUT",
                "repos/acme/widget/pulls/12/merge",
                "-f",
                "merge_method=merge",
            ],
            0,
            r#"{"merged": true}"#,
            "",
        );
        let repo = repo(&scene, "github", "acme/widget");
        assert_eq!(pr::request_auto_merge(&repo, &twelve(&scene)), Ok(()));
        assert!(was_asked(&merged));
    }

    #[test]
    fn a_github_auto_merge_the_forge_refuses_is_reported_in_its_own_words() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("am-refused.test");
        settings(&scene, [true, true, false, false]);
        enable(
            &scene,
            "REBASE",
            1,
            "",
            "gh: Resource not accessible by integration\n",
        );
        let repo = repo(&scene, "github", "acme/widget");
        let said = pr::request_auto_merge(&repo, &twelve(&scene)).unwrap_err();
        assert!(
            said.contains("Resource not accessible by integration"),
            "{said}"
        );
    }

    fn gitlab_project(scene: &Scene, squash_option: &str) {
        scene.glab_api(
            "projects/acme%2Fplat%2Fwidget",
            0,
            &format!(r#"{{"merge_method": "rebase_merge", "squash_option": "{squash_option}"}}"#),
            "",
        );
    }

    fn gitlab_merge(scene: &Scene, squash: &str) -> PathBuf {
        scene.answers(
            "glab",
            &[
                "--hostname",
                &scene.host,
                "api",
                "-X",
                "PUT",
                "projects/acme%2Fplat%2Fwidget/merge_requests/3/merge",
                "-F",
                "merge_when_pipeline_succeeds=true",
                "-F",
                &format!("squash={squash}"),
            ],
            0,
            r#"{"iid": 3, "state": "opened", "merge_when_pipeline_succeeds": true}"#,
            "",
        )
    }

    fn three(scene: &Scene) -> Pr {
        Pr {
            number: 3,
            url: format!("https://{}/acme/plat/widget/-/merge_requests/3", scene.host),
        }
    }

    #[test]
    fn gitlab_auto_merge_merges_by_the_projects_method_without_squashing_it_may_avoid() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("mr-am.test");
        gitlab_project(&scene, "default_on");
        let asked = gitlab_merge(&scene, "false");
        let repo = repo(&scene, "gitlab", "acme/plat/widget");
        assert_eq!(pr::request_auto_merge(&repo, &three(&scene)), Ok(()));
        assert!(was_asked(&asked));
    }

    #[test]
    fn gitlab_auto_merge_squashes_when_the_project_always_squashes() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("mr-am-always.test");
        gitlab_project(&scene, "always");
        let asked = gitlab_merge(&scene, "true");
        let repo = repo(&scene, "gitlab", "acme/plat/widget");
        assert_eq!(pr::request_auto_merge(&repo, &three(&scene)), Ok(()));
        assert!(was_asked(&asked));
    }

    // -- which repo ------------------------------------------------------------------------ //

    fn clone_with_origin(plane: &std::path::Path, origin: &str) -> PathBuf {
        let clone = plane.join("workspaces/alpha/widget");
        std::fs::create_dir_all(&clone).unwrap();
        for args in [
            vec!["init", "-q", "-b", "main", "."],
            vec!["remote", "add", "origin", origin],
        ] {
            let done =
                charter_core::forklock::output(support::unsigned().args(&args).current_dir(&clone))
                    .unwrap();
            assert!(done.status.success(), "{done:?}");
        }
        clone
    }

    #[test]
    fn a_clones_repo_is_its_origins_forge_and_path() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(
            plane.join("charter.toml"),
            "schema = 1\n\n[[forge]]\nkind = \"gitlab\"\nhost = \"git.internal\"\n",
        )
        .unwrap();
        let clone = clone_with_origin(&plane, "git@git.internal:acme/plat/widget.git");
        assert_eq!(
            Repo::of_clone(&plane, &clone),
            Ok(Repo {
                forge: charter_core::forge::Forge::build("gitlab", Some("git.internal")).unwrap(),
                path: "acme/plat/widget".into(),
            })
        );
    }

    #[test]
    fn a_clone_whose_origin_is_on_no_forge_the_plane_knows_has_no_pull_requests() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();
        let clone = clone_with_origin(&plane, "https://git.example.org/acme/widget.git");
        let said = Repo::of_clone(&plane, &clone).unwrap_err();
        assert!(said.contains("git.example.org"), "{said}");
    }

    #[test]
    fn a_clone_with_no_origin_or_an_origin_naming_no_repo_has_no_pull_requests() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();
        let bare = clone_with_origin(&plane, "https://github.com/");
        let said = Repo::of_clone(&plane, &bare).unwrap_err();
        assert!(said.contains("names no repository"), "{said}");

        let none = plane.join("workspaces/alpha/lonely");
        std::fs::create_dir_all(&none).unwrap();
        let done = charter_core::forklock::output(
            support::unsigned()
                .args(["init", "-q", "."])
                .current_dir(&none),
        )
        .unwrap();
        assert!(done.status.success(), "{done:?}");
        let said = Repo::of_clone(&plane, &none).unwrap_err();
        assert!(said.contains("has no origin"), "{said}");
    }
}
