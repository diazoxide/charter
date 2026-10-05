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

use charter_core::forge::Caller;
use charter_core::forge::pr::{self, AutoMerge, Opened, Pr, Repo, State};

/// The seam's `open_or_update`, asked of `repo`'s own backend over the CLI transport, with
/// the error as its words.
fn open_or_update(
    repo: &Repo,
    head: &str,
    base: &str,
    title: &str,
    body: &str,
) -> Result<Opened, String> {
    repo.backend()
        .open_or_update(&Caller::command(), &repo.path, head, base, title, body)
        .map_err(|e| e.to_string())
}

/// The seam's `request_auto_merge`, as above.
fn request_auto_merge(repo: &Repo, pr: &Pr, head: &str) -> Result<AutoMerge, String> {
    repo.backend()
        .request_auto_merge(&Caller::command(), &repo.path, pr, head)
        .map_err(|e| e.to_string())
}

/// The seam's `state`, as above.
fn state(repo: &Repo, pr: &Pr) -> Result<State, String> {
    repo.backend()
        .state(&Caller::command(), &repo.path, pr)
        .map_err(|e| e.to_string())
}
use support::forge_cli::{Scene, in_a_child, in_child, was_asked};

#[test]
fn every_pull_request_question_is_asked_of_a_stand_in_cli() {
    charter_core::unsteered!();
    in_a_child("prs::", "bin");
}

/// The one GraphQL document that reads what a GitHub repo allows and the PR's node id.
const SETTINGS: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){autoMergeAllowed rebaseMergeAllowed mergeCommitAllowed squashMergeAllowed pullRequest(number:$number){id}}}";

/// The one GraphQL document that turns auto-merge on.
const ENABLE: &str = "mutation($id:ID!,$method:PullRequestMergeMethod!,$head:GitObjectID!){enablePullRequestAutoMerge(input:{pullRequestId:$id,mergeMethod:$method,expectedHeadOid:$head}){clientMutationId}}";

/// The commit this save pushed, which is the only head the forge may merge.
const PUSHED: &str = "0123456789abcdef0123456789abcdef01234567";

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
        let opened = open_or_update(
            &repo(&scene, "github", "acme/widget"),
            "charter/save/mac",
            "release",
            "Save from mac",
            "memory: 2 files\n@not-a-file",
        )
        .map(|opened| opened.pr);
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
        let opened = open_or_update(
            &repo(&scene, "github", "acme/widget"),
            "charter/save/mac",
            "release",
            "Save from mac",
            "newer",
        )
        .map(|opened| opened.pr);
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
    fn a_pr_a_person_opened_from_the_same_branch_is_left_as_it_is() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        // Their own branch, their own words: no PATCH is written down, so one would fail.
        let scene = Scene::new("pr-theirs.test");
        scene.gh_api(
            "repos/acme/widget/pulls?state=open&head=acme:feature%2Fx&base=main&per_page=1",
            0,
            r#"[{"number": 9, "html_url": "https://pr-theirs.test/acme/widget/pull/9", "body": "Hand-written."}]"#,
            "",
        );
        let opened = open_or_update(
            &repo(&scene, "github", "acme/widget"),
            "feature/x",
            "main",
            "charter save: 1 file",
            "body",
        );
        assert_eq!(
            opened,
            Ok(pr::Opened {
                pr: Pr {
                    number: 9,
                    url: "https://pr-theirs.test/acme/widget/pull/9".into()
                },
                ours: false
            })
        );
    }

    #[test]
    fn a_pr_whose_body_carries_charters_marker_is_charters_to_update() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("pr-marked.test");
        scene.gh_api(
            "repos/acme/widget/pulls?state=open&head=acme:feature%2Fx&base=main&per_page=1",
            0,
            &format!(
                r#"[{{"number": 5, "html_url": "https://pr-marked.test/acme/widget/pull/5", "body": "Saved.\n\n{}"}}]"#,
                pr::MARKER
            ),
            "",
        );
        let updated = scene.answers(
            "gh",
            &[
                "api",
                "--hostname",
                "pr-marked.test",
                "-X",
                "PATCH",
                "repos/acme/widget/pulls/5",
                "-f",
                "title=t",
                "-f",
                "body=b",
            ],
            0,
            r#"{"number": 5, "html_url": "https://pr-marked.test/acme/widget/pull/5"}"#,
            "",
        );
        let opened = open_or_update(
            &repo(&scene, "github", "acme/widget"),
            "feature/x",
            "main",
            "t",
            "b",
        );
        assert_eq!(opened.map(|o| (o.pr.number, o.ours)), Ok((5, true)));
        assert!(was_asked(&updated));
    }

    #[test]
    fn a_pr_whose_body_carries_the_marker_charter_wrote_before_the_rename_is_still_its_own() {
        charter_core::unsteered!();
        // V93j: bodies already on a forge keep `<!-- charter-save -->`, recognised forever.
        if !in_child() {
            return;
        }
        let scene = Scene::new("pr-marked-old.test");
        scene.gh_api(
            "repos/acme/widget/pulls?state=open&head=acme:feature%2Fx&base=main&per_page=1",
            0,
            &format!(
                r#"[{{"number": 5, "html_url": "https://pr-marked-old.test/acme/widget/pull/5", "body": "Saved.\n\n{}"}}]"#,
                "<!-- charter-save -->"
            ),
            "",
        );
        let updated = scene.answers(
            "gh",
            &[
                "api",
                "--hostname",
                "pr-marked-old.test",
                "-X",
                "PATCH",
                "repos/acme/widget/pulls/5",
                "-f",
                "title=t",
                "-f",
                "body=b",
            ],
            0,
            r#"{"number": 5, "html_url": "https://pr-marked-old.test/acme/widget/pull/5"}"#,
            "",
        );
        let opened = open_or_update(
            &repo(&scene, "github", "acme/widget"),
            "feature/x",
            "main",
            "t",
            "b",
        );
        assert_eq!(opened.map(|o| (o.pr.number, o.ours)), Ok((5, true)));
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
        let said = open_or_update(
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
        let said = open_or_update(
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
            "projects/acme%2Fplat%2Fwidget/merge_requests?state=opened&source_branch=charter%2Fsave%2Fmac&target_branch=release&per_page=100",
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
        let opened = open_or_update(
            &repo(&scene, "gitlab", "acme/plat/widget"),
            "charter/save/mac",
            "release",
            "Save from mac",
            "body",
        )
        .map(|opened| opened.pr);
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
            r#"[{"iid": 9, "source_project_id": 77, "target_project_id": 5, "web_url": "https://mr-update.test/acme/plat/widget/-/merge_requests/9"},
                {"iid": 4, "source_project_id": 5, "target_project_id": 5, "web_url": "https://mr-update.test/acme/plat/widget/-/merge_requests/4"}]"#,
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
        let opened = open_or_update(
            &repo(&scene, "gitlab", "acme/plat/widget"),
            "charter/save/mac",
            "release",
            "Save from mac",
            "newer",
        )
        .map(|opened| opened.pr);
        assert_eq!(opened.map(|p| p.number), Ok(4));
        assert!(was_asked(&updated));
    }

    #[test]
    fn a_github_description_is_read_whole_and_written_as_one_literal_field() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("pr-body.test");
        let read = scene.gh_api(
            "repos/acme/widget/pulls/12",
            0,
            r#"{"number": 12, "body": null}"#,
            "",
        );
        // A description is somebody's text: `-f`, never `-F`, so a leading `@` is not a file
        // to upload (charter #323), and nothing but the body is sent.
        let wrote = scene.answers(
            "gh",
            &[
                "api",
                "--hostname",
                "pr-body.test",
                "-X",
                "PATCH",
                "repos/acme/widget/pulls/12",
                "-f",
                "body=@not-a-file\nmore",
            ],
            0,
            r#"{"number": 12}"#,
            "",
        );
        let repo = repo(&scene, "github", "acme/widget");
        let pr = Pr {
            number: 12,
            url: String::new(),
        };
        let backend = repo.backend();
        assert_eq!(
            backend.body(&Caller::command(), &repo.path, &pr),
            Ok(String::new()),
            "a null body is empty"
        );
        assert_eq!(
            backend.set_body(&Caller::command(), &repo.path, &pr, "@not-a-file\nmore"),
            Ok(())
        );
        assert!(was_asked(&read) && was_asked(&wrote));
    }

    #[test]
    fn a_gitlab_description_is_read_whole_and_written_as_one_literal_field() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("mr-body.test");
        let read = scene.glab_api(
            "projects/acme%2Fplat%2Fwidget/merge_requests/4",
            0,
            r#"{"iid": 4, "description": "theirs"}"#,
            "",
        );
        let wrote = scene.answers(
            "glab",
            &[
                "--hostname",
                "mr-body.test",
                "api",
                "-X",
                "PUT",
                "projects/acme%2Fplat%2Fwidget/merge_requests/4",
                "-f",
                "description=@not-a-file",
            ],
            0,
            r#"{"iid": 4}"#,
            "",
        );
        let repo = repo(&scene, "gitlab", "acme/plat/widget");
        let pr = Pr {
            number: 4,
            url: String::new(),
        };
        let backend = repo.backend();
        assert_eq!(
            backend.body(&Caller::command(), &repo.path, &pr),
            Ok("theirs".to_string())
        );
        assert_eq!(
            backend.set_body(&Caller::command(), &repo.path, &pr, "@not-a-file"),
            Ok(())
        );
        assert!(was_asked(&read) && was_asked(&wrote));
    }

    #[test]
    fn a_gitlab_mr_from_a_fork_with_the_same_branch_name_is_never_adopted() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("mr-fork.test");
        gitlab_lookup(
            &scene,
            r#"[{"iid": 9, "source_project_id": 77, "target_project_id": 5, "web_url": "https://mr-fork.test/acme/plat/widget/-/merge_requests/9"}]"#,
        );
        let created = scene.answers(
            "glab",
            &[
                "--hostname",
                "mr-fork.test",
                "api",
                "-X",
                "POST",
                "projects/acme%2Fplat%2Fwidget/merge_requests",
                "-f",
                "source_branch=charter/save/mac",
                "-f",
                "target_branch=release",
                "-f",
                "title=t",
                "-f",
                "description=b",
            ],
            0,
            r#"{"iid": 10, "web_url": "https://mr-fork.test/acme/plat/widget/-/merge_requests/10"}"#,
            "",
        );
        let opened = open_or_update(
            &repo(&scene, "gitlab", "acme/plat/widget"),
            "charter/save/mac",
            "release",
            "t",
            "b",
        )
        .map(|opened| opened.pr);
        assert_eq!(
            opened.map(|p| p.number),
            Ok(10),
            "a new MR, not the fork's !9"
        );
        assert!(was_asked(&created));
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
                "-f",
                &format!("head={PUSHED}"),
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
        assert_eq!(
            request_auto_merge(&repo, &twelve(&scene), PUSHED),
            Ok(AutoMerge::Queued)
        );
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
        assert_eq!(
            request_auto_merge(&repo, &twelve(&scene), PUSHED),
            Ok(AutoMerge::Queued)
        );
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
        assert_eq!(
            request_auto_merge(&repo, &twelve(&scene), PUSHED),
            Ok(AutoMerge::Queued)
        );
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
        let said = request_auto_merge(&repo, &twelve(&scene), PUSHED).unwrap_err();
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
            request_auto_merge(&repo, &twelve(&scene), PUSHED),
            Err("acme/widget allows no merge method".into())
        );
    }

    #[test]
    fn a_github_pr_with_nothing_left_to_wait_for_is_not_queued_and_not_merged() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        // No merge question is written down, so a merge would fail the call.
        for (host, status) in [("am-clean.test", "clean"), ("am-unstable.test", "unstable")] {
            let scene = Scene::new(host);
            settings(&scene, [true, false, true, true]);
            let said = format!("gh: Pull request Pull request is in {status} status\n");
            enable(
                &scene,
                "MERGE",
                1,
                r#"{"data": {"enablePullRequestAutoMerge": null}, "errors": [{"type": "UNPROCESSABLE"}]}"#,
                &said,
            );
            let repo = repo(&scene, "github", "acme/widget");
            assert_eq!(
                request_auto_merge(&repo, &twelve(&scene), PUSHED),
                Ok(AutoMerge::NotQueued(format!(
                    "gh: Pull request Pull request is in {status} status"
                ))),
                "{status}"
            );
        }
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
        let said = request_auto_merge(&repo, &twelve(&scene), PUSHED).unwrap_err();
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

    fn gitlab_mr(scene: &Scene, pipeline: &str) {
        scene.glab_api(
            "projects/acme%2Fplat%2Fwidget/merge_requests/3",
            0,
            &format!(r#"{{"iid": 3, "sha": "{PUSHED}", "head_pipeline": {pipeline}}}"#),
            "",
        );
    }

    fn gitlab_merge(scene: &Scene, squash: &str) -> PathBuf {
        gitlab_merge_answered(
            scene,
            squash,
            0,
            r#"{"iid": 3, "state": "opened", "merge_when_pipeline_succeeds": true}"#,
            "",
        )
    }

    fn gitlab_merge_answered(
        scene: &Scene,
        squash: &str,
        code: i32,
        out: &str,
        err: &str,
    ) -> PathBuf {
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
                "-f",
                &format!("sha={PUSHED}"),
            ],
            code,
            out,
            err,
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
        gitlab_mr(&scene, r#"{"status": "running"}"#);
        let asked = gitlab_merge(&scene, "false");
        let repo = repo(&scene, "gitlab", "acme/plat/widget");
        assert_eq!(
            request_auto_merge(&repo, &three(&scene), PUSHED),
            Ok(AutoMerge::Queued)
        );
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
        gitlab_mr(&scene, r#"{"status": "pending"}"#);
        let asked = gitlab_merge(&scene, "true");
        let repo = repo(&scene, "gitlab", "acme/plat/widget");
        assert_eq!(
            request_auto_merge(&repo, &three(&scene), PUSHED),
            Ok(AutoMerge::Queued)
        );
        assert!(was_asked(&asked));
    }

    #[test]
    fn a_gitlab_mr_with_no_pipeline_running_is_not_queued_and_not_merged() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        // GitLab merges at once when told to wait for a pipeline that is not running, so
        // charter does not ask; no merge question is written down.
        for (host, pipeline) in [
            ("mr-none.test", "null"),
            ("mr-done.test", r#"{"status": "success"}"#),
        ] {
            let scene = Scene::new(host);
            gitlab_project(&scene, "default_off");
            gitlab_mr(&scene, pipeline);
            let repo = repo(&scene, "gitlab", "acme/plat/widget");
            let got = request_auto_merge(&repo, &three(&scene), PUSHED);
            let Ok(AutoMerge::NotQueued(why)) = got else {
                panic!("{host}: {got:?}");
            };
            assert!(why.contains("no pipeline running"), "{why}");
        }
    }

    #[test]
    fn a_gitlab_merge_refused_as_not_mergeable_is_not_queued_and_other_refusals_are_errors() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        for (host, code_said, queued) in [
            (
                "mr-405.test",
                "glab: 405 Method Not Allowed (HTTP 405)",
                true,
            ),
            (
                "mr-422.test",
                "glab: 422 Unprocessable Entity (HTTP 422)",
                true,
            ),
            (
                "mr-409.test",
                "glab: 409 SHA does not match HEAD of source branch (HTTP 409)",
                false,
            ),
        ] {
            let scene = Scene::new(host);
            gitlab_project(&scene, "never");
            gitlab_mr(&scene, r#"{"status": "running"}"#);
            gitlab_merge_answered(&scene, "false", 1, "", code_said);
            let repo = repo(&scene, "gitlab", "acme/plat/widget");
            let got = request_auto_merge(&repo, &three(&scene), PUSHED);
            if queued {
                assert_eq!(got, Ok(AutoMerge::NotQueued(code_said.into())), "{host}");
            } else {
                assert!(got.unwrap_err().contains("SHA does not match"), "{host}");
            }
        }
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
    fn a_planes_repo_is_its_own_origins() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();
        for args in [
            vec!["init", "-q", "-b", "main", "."],
            vec!["remote", "add", "origin", "git@github.com:acme/plane.git"],
        ] {
            let done =
                charter_core::forklock::output(support::unsigned().args(&args).current_dir(&plane))
                    .unwrap();
            assert!(done.status.success(), "{done:?}");
        }
        assert_eq!(
            Repo::of_plane(&plane),
            Ok(Repo {
                forge: charter_core::forge::Forge::build("github", None).unwrap(),
                path: "acme/plane".into(),
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

    #[test]
    fn a_github_prs_state_is_open_merged_or_closed_without_merging() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("pr-state.test");
        let repo = repo(&scene, "github", "acme/widget");
        for (number, answer, want) in [
            (
                1,
                r#"{"state": "open", "merged": false, "merged_at": null}"#,
                State::Open,
            ),
            (
                2,
                r#"{"state": "closed", "merged": true, "merged_at": "2026-09-25T00:00:00Z", "merge_commit_sha": "abc123"}"#,
                State::Merged {
                    commit: Some("abc123".into()),
                },
            ),
            (
                3,
                r#"{"state": "closed", "merged": false, "merged_at": null}"#,
                State::Closed,
            ),
        ] {
            scene.gh_api(&format!("repos/acme/widget/pulls/{number}"), 0, answer, "");
            let pr = Pr {
                number,
                url: format!("https://pr-state.test/acme/widget/pull/{number}"),
            };
            assert_eq!(state(&repo, &pr), Ok(want), "#{number}");
        }
    }

    #[test]
    fn a_gitlab_mrs_state_is_open_merged_or_closed_without_merging() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("mr-state.test");
        let repo = repo(&scene, "gitlab", "acme/plat/widget");
        for (number, answer, want) in [
            (1, r#"{"iid": 1, "state": "opened"}"#, State::Open),
            (
                2,
                r#"{"iid": 2, "state": "merged", "merge_commit_sha": "m1", "squash_commit_sha": null}"#,
                State::Merged {
                    commit: Some("m1".into()),
                },
            ),
            (
                4,
                r#"{"iid": 4, "state": "merged", "merge_commit_sha": null, "squash_commit_sha": "s1"}"#,
                State::Merged {
                    commit: Some("s1".into()),
                },
            ),
            (
                5,
                r#"{"iid": 5, "state": "merged", "merge_commit_sha": null, "squash_commit_sha": null}"#,
                State::Merged { commit: None },
            ),
            (3, r#"{"iid": 3, "state": "closed"}"#, State::Closed),
        ] {
            scene.glab_api(
                &format!("projects/acme%2Fplat%2Fwidget/merge_requests/{number}"),
                0,
                answer,
                "",
            );
            let pr = Pr {
                number,
                url: format!("https://mr-state.test/acme/plat/widget/-/merge_requests/{number}"),
            };
            assert_eq!(state(&repo, &pr), Ok(want), "!{number}");
        }
    }

    #[test]
    fn a_state_the_forge_would_not_give_is_an_error_and_never_read_as_open() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("pr-state-fails.test");
        let repo = repo(&scene, "github", "acme/widget");
        scene.gh_api("repos/acme/widget/pulls/4", 1, "", "HTTP 502: Bad Gateway");
        scene.gh_api("repos/acme/widget/pulls/5", 0, r#"{"state": "draft"}"#, "");
        let four = Pr {
            number: 4,
            url: "x".into(),
        };
        let five = Pr {
            number: 5,
            url: "y".into(),
        };
        assert!(state(&repo, &four).unwrap_err().contains("HTTP 502"));
        assert!(state(&repo, &five).unwrap_err().contains("draft"));
    }

    // -- charter change land: merging now, or through the queue, at one head --------------- //

    fn landing() -> pr::MergeAs {
        pr::MergeAs {
            squash: false,
            title: "@api-2: widget (#12)".into(),
            message: "bump the api\n\nCharter-Change: api-2".into(),
        }
    }

    #[test]
    fn a_github_landing_merges_by_rest_with_sha_and_charters_message_as_literal_fields() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("land-gh.test");
        // The subject and body are `-f`, so a leading `@` is text, not a file to upload.
        let merged = scene.answers(
            "gh",
            &[
                "api",
                "--hostname",
                "land-gh.test",
                "-X",
                "PUT",
                "repos/acme/widget/pulls/12/merge",
                "-f",
                "merge_method=merge",
                "-f",
                "commit_title=@api-2: widget (#12)",
                "-f",
                "commit_message=bump the api\n\nCharter-Change: api-2",
                "-f",
                &format!("sha={PUSHED}"),
            ],
            0,
            r#"{"sha": "e5bd3914e2e596debea16f433f57875b5b90bcd6", "merged": true}"#,
            "",
        );
        let repo = repo(&scene, "github", "acme/widget");
        assert_eq!(
            repo.backend().merge_at(
                &Caller::command(),
                &repo.path,
                &twelve(&scene),
                PUSHED,
                &landing()
            ),
            Ok(pr::MergedAt::Now)
        );
        assert!(was_asked(&merged));
    }

    #[test]
    fn a_github_landing_through_the_merge_queue_is_pinned_with_expected_head_oid() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("land-mq.test");
        let probe = include_str!("../src/forge/github/queries/merge_queue.graphql");
        let enqueue = include_str!("../src/forge/github/queries/enqueue.graphql");
        let probed = scene.answers(
            "gh",
            &[
                "api",
                "graphql",
                "--hostname",
                "land-mq.test",
                "-f",
                &format!("query={probe}"),
                "-f",
                "owner=acme",
                "-f",
                "name=widget",
                "-F",
                "number=12",
            ],
            0,
            r#"{"data": {"repository": {"pullRequest": {"id": "PR_node12", "isMergeQueueEnabled": true}}}}"#,
            "",
        );
        let queued = scene.answers(
            "gh",
            &[
                "api",
                "graphql",
                "--hostname",
                "land-mq.test",
                "-f",
                &format!("query={enqueue}"),
                "-f",
                "id=PR_node12",
                "-f",
                &format!("head={PUSHED}"),
            ],
            0,
            &format!(
                r#"{{"data": {{"enqueuePullRequest": {{"mergeQueueEntry": {{"state": "QUEUED", "headCommit": {{"oid": "{PUSHED}"}}}}}}}}}}"#
            ),
            "",
        );
        let repo = repo(&scene, "github", "acme/widget");
        assert_eq!(
            repo.backend().enqueue_at(
                &Caller::command(),
                &repo.path,
                &twelve(&scene),
                PUSHED,
                &landing()
            ),
            Ok(())
        );
        assert!(was_asked(&probed) && was_asked(&queued));
    }

    #[test]
    fn a_gitlab_landing_sends_sha_and_never_merge_when_pipeline_succeeds() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("land-gl.test");
        // The whole argv: a `merge_when_pipeline_succeeds` or `auto_merge` anywhere in it is a
        // question nobody wrote down, which the stand-in answers with exit 99.
        let merged = scene.answers(
            "glab",
            &[
                "--hostname",
                "land-gl.test",
                "api",
                "-X",
                "PUT",
                "projects/acme%2Fplat%2Fwidget/merge_requests/3/merge",
                "-F",
                "squash=false",
                "-f",
                &format!("sha={PUSHED}"),
                "-f",
                "merge_commit_message=@api-2: widget (#12)\n\nbump the api\n\nCharter-Change: api-2",
            ],
            0,
            r#"{"iid": 3, "state": "merged", "merge_commit_sha": "e5bd3914e2e596debea16f433f57875b5b90bcd6"}"#,
            "",
        );
        let trained = scene.answers(
            "glab",
            &[
                "--hostname",
                "land-gl.test",
                "api",
                "-X",
                "POST",
                "projects/acme%2Fplat%2Fwidget/merge_trains/merge_requests/3",
                "-F",
                "squash=false",
                "-f",
                &format!("sha={PUSHED}"),
            ],
            0,
            r#"[{"id": 1, "merge_request": {"iid": 3}, "status": "idle"}]"#,
            "",
        );
        let repo = repo(&scene, "gitlab", "acme/plat/widget");
        let backend = repo.backend();
        assert_eq!(
            backend.merge_at(
                &Caller::command(),
                &repo.path,
                &three(&scene),
                PUSHED,
                &landing()
            ),
            Ok(pr::MergedAt::Now)
        );
        assert_eq!(
            backend.enqueue_at(
                &Caller::command(),
                &repo.path,
                &three(&scene),
                PUSHED,
                &landing()
            ),
            Ok(())
        );
        assert!(was_asked(&merged) && was_asked(&trained));
    }
}
