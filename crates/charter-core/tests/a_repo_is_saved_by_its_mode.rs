//! A workspace repo saved in the pull request modes (charter-app#299, ADR 0051): pushed to the
//! branch it is on — or, on the default branch, to `charter/<workspace>/<short-sha>` — and a
//! pull request opened or updated from it.
//!
//! The push reaches a bare repository in the test's own directory through an `insteadOf` in
//! the clone's config, keyed on the HTTPS base charter builds (the arrangement
//! `planegit::origin_https` documents). Every forge question goes to a stand-in `gh` that
//! answers only what a test wrote down, exactly once (`support/forge_cli.rs`). Nothing here
//! reaches a network.

mod support;

use std::path::{Path, PathBuf};

use charter_core::planegit::{self, Stage, Trigger};
use charter_core::repocmd::Say;
use charter_core::reposave::{self, Request};
use support::forge_cli::{Scene, in_a_child, in_child, was_asked};

#[test]
fn every_repo_save_question_is_asked_of_a_stand_in_cli() {
    charter_core::unsteered!();
    in_a_child("repo_saves::", "bin");
}

mod repo_saves {
    use super::*;

    struct Repo {
        plane: PathBuf,
        clone: PathBuf,
        bare: PathBuf,
        _fixture: support::Fixture,
    }

    /// A plane declaring `host` as a GitHub, saying `toml` about `widget`, whose inventory
    /// names `main` as widget's default branch; and a clone `alpha/widget` on `main`, with its
    /// one commit on the stand-in remote.
    fn repo(host: &str, toml: &str) -> Repo {
        let f = support::plane_with_clone("widget");
        assert_eq!((f.ws.as_str(), f.repo.as_str()), ("alpha", "widget"));
        std::fs::write(
            f.plane.join("charter.toml"),
            format!("[[forge]]\nkind = \"github\"\nhost = \"{host}\"\n\n{toml}"),
        )
        .unwrap();
        std::fs::create_dir_all(f.plane.join("inventory")).unwrap();
        std::fs::write(
            f.plane.join("inventory/repos.json"),
            r#"{"repos": [{"name": "widget", "default_branch": "main"}]}"#,
        )
        .unwrap();
        let clone = f.clone.clone();
        support::git(&clone, &["config", "user.name", "charter tests"]);
        support::git(&clone, &["config", "user.email", "tests@example.invalid"]);
        // Inside the fixture's own directory, which goes when the test does.
        let bare = f.plane.join(".test-forge/acme/widget.git");
        std::fs::create_dir_all(&bare).unwrap();
        support::git(&bare, &["init", "-q", "--bare", "-b", "main", "."]);
        support::git(
            &clone,
            &[
                "remote",
                "add",
                "origin",
                &format!("git@{host}:acme/widget.git"),
            ],
        );
        let base = format!("file://{}/", bare.parent().unwrap().display());
        support::git(
            &clone,
            &[
                "config",
                &format!("url.{base}.insteadOf"),
                &format!("https://{host}/acme/"),
            ],
        );
        let at = bare.display().to_string();
        support::git(&clone, &["push", "-q", &at, "main"]);
        support::git(
            &clone,
            &["fetch", "-q", &at, "+refs/heads/*:refs/remotes/origin/*"],
        );
        Repo {
            plane: f.plane.clone(),
            clone,
            bare,
            _fixture: f,
        }
    }

    impl Repo {
        fn save(&self) -> (u8, String) {
            let mut said = String::new();
            let mut say = |line: Say| {
                said.push_str(&line.to_string());
                said.push('\n');
            };
            let code = reposave::save_as(
                &Request {
                    plane: &self.plane,
                    workspace: "alpha",
                    name: "widget",
                    clone: &self.clone,
                    message: None,
                    no_push: false,
                    mid_turn: &reposave::nobody_working,
                },
                Trigger::Manual,
                &mut say,
            );
            (code, said)
        }

        fn head(&self) -> String {
            rev(&self.clone, "HEAD")
        }

        fn remote(&self, branch: &str) -> String {
            rev(&self.bare, &format!("refs/heads/{branch}"))
        }

        fn commit(&self, file: &str, subject: &str) {
            std::fs::write(self.clone.join(file), subject).unwrap();
            support::git(&self.clone, &["add", "-A"]);
            support::git(&self.clone, &["commit", "-q", "-m", subject]);
        }

        fn standing(&self) -> reposave::Standing {
            reposave::standing(
                &self.plane,
                "alpha",
                &charter_core::repos::Repo {
                    name: "widget".into(),
                    path: self.clone.clone(),
                },
            )
        }

        fn last(&self) -> serde_json::Value {
            planegit::journal(&self.plane)
                .pop()
                .expect("a journal line")
        }
    }

    fn rev(dir: &Path, what: &str) -> String {
        String::from_utf8(support::git(dir, &["rev-parse", what]).stdout)
            .unwrap()
            .trim()
            .to_string()
    }

    const BODY: &str = "body=Saved by charter from the alpha workspace ([repos.widget] mode = pr).\n\n<!-- charter-save -->";

    fn lookup(scene: &Scene, head: &str, out: &str) -> PathBuf {
        scene.gh_api(
            &format!(
                "repos/acme/widget/pulls?state=open&head=acme:{}&base=main&per_page=1",
                head.replace('/', "%2F")
            ),
            0,
            out,
            "",
        )
    }

    fn create(scene: &Scene, head: &str, title: &str, body: &str, number: u32) -> PathBuf {
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
                &format!("head={head}"),
                "-f",
                "base=main",
                "-f",
                &format!("title={title}"),
                "-f",
                body,
            ],
            0,
            &format!(
                r#"{{"number": {number}, "html_url": "https://{}/acme/widget/pull/{number}"}}"#,
                scene.host
            ),
            "",
        )
    }

    #[test]
    fn a_feature_branch_in_pr_mode_is_pushed_as_itself_and_a_pr_opened_into_the_default() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let host = "repo-feature.test";
        let scene = Scene::new(host);
        let r = repo(host, "");
        let main_before = r.remote("main");
        support::git(&r.clone, &["checkout", "-q", "-b", "feature/x"]);
        std::fs::write(r.clone.join("src.rs"), "fn main() {}\n").unwrap();
        let looked = lookup(&scene, "feature/x", "[]");
        let opened = create(
            &scene,
            "feature/x",
            "charter save: 1 file (src.rs)",
            BODY,
            3,
        );

        let (code, said) = r.save();

        assert_eq!(code, 0, "{said}");
        assert!(was_asked(&looked) && was_asked(&opened), "{said}");
        assert_eq!(r.remote("feature/x"), r.head());
        assert_eq!(r.remote("main"), main_before, "main was pushed");
        let line = r.last();
        assert_eq!(line["target"], "repo:alpha/widget");
        assert_eq!(line["mode"], "pr");
        assert_eq!(line["outcome"], "pr-open");
        assert_eq!(line["branch"], "feature/x");
        assert_eq!(line["pr"], format!("https://{host}/acme/widget/pull/3"));
        let standing = r.standing();
        assert_eq!(standing.stage, Stage::PrOpen);
        assert_eq!(
            standing.pr,
            Some(format!("https://{host}/acme/widget/pull/3"))
        );
    }

    #[test]
    fn on_the_default_branch_pr_mode_pushes_a_branch_of_its_own_and_never_main() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let host = "repo-default.test";
        let scene = Scene::new(host);
        let r = repo(host, "[repos.widget]\nmode = \"pr\"\n");
        let main_before = r.remote("main");
        r.commit("a.md", "first change");
        let short = &r.head()[..7];
        let branch = format!("charter/alpha/{short}");
        let first_lookup = lookup(&scene, &branch, "[]");
        let opened = create(&scene, &branch, "first change", BODY, 4);

        let (code, said) = r.save();

        assert_eq!(code, 0, "{said}");
        assert!(was_asked(&opened), "{said}");
        assert_eq!(r.remote(&branch), r.head());
        assert_eq!(r.remote("main"), main_before, "main was pushed: {said}");
        let on = String::from_utf8(
            support::git(&r.clone, &["rev-parse", "--abbrev-ref", "HEAD"]).stdout,
        )
        .unwrap();
        assert_eq!(on.trim(), "main", "the clone was moved off main");
        assert_eq!(rev(&r.clone, &format!("refs/heads/{branch}")), r.head());
        assert_eq!(r.last()["branch"], branch.as_str());

        // The next save from main carries on in the same branch, and updates the same PR. The
        // stand-in answers a question once, and this lookup is the same question again, so
        // the first one's is taken down.
        std::fs::remove_file(first_lookup.with_extension("args")).unwrap();
        r.commit("b.md", "second change");
        let found =
            format!(r#"[{{"number": 4, "html_url": "https://{host}/acme/widget/pull/4"}}]"#);
        lookup(&scene, &branch, &found);
        let updated = scene.answers(
            "gh",
            &[
                "api",
                "--hostname",
                host,
                "-X",
                "PATCH",
                "repos/acme/widget/pulls/4",
                "-f",
                "title=second change",
                "-f",
                BODY,
            ],
            0,
            &format!(r#"{{"number": 4, "html_url": "https://{host}/acme/widget/pull/4"}}"#),
            "",
        );

        let (code, said) = r.save();

        assert_eq!(code, 0, "{said}");
        assert!(was_asked(&updated), "{said}");
        assert_eq!(r.remote(&branch), r.head());
        assert_eq!(r.remote("main"), main_before);
        assert_eq!(r.standing().stage, Stage::PrOpen);
    }

    #[test]
    fn pr_merge_that_the_forge_will_not_queue_says_why_and_leaves_the_pr_open() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let host = "repo-merge.test";
        let scene = Scene::new(host);
        let r = repo(host, "[repos.widget]\nmode = \"pr-merge\"\n");
        support::git(&r.clone, &["checkout", "-q", "-b", "feature/y"]);
        r.commit("c.md", "a change");
        let body = "body=Saved by charter from the alpha workspace ([repos.widget] mode = pr-merge).\n\n<!-- charter-save -->";
        lookup(&scene, "feature/y", "[]");
        create(&scene, "feature/y", "a change", body, 12);
        scene.answers(
            "gh",
            &[
                "api",
                "graphql",
                "--hostname",
                host,
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
            r#"{"data": {"repository": {"autoMergeAllowed": true, "rebaseMergeAllowed": true, "mergeCommitAllowed": true, "squashMergeAllowed": true, "pullRequest": {"id": "PR_node12"}}}}"#,
            "",
        );
        let enable = scene.answers(
            "gh",
            &[
                "api",
                "graphql",
                "--hostname",
                host,
                "-f",
                &format!("query={ENABLE}"),
                "-f",
                "id=PR_node12",
                "-f",
                "method=REBASE",
                "-f",
                &format!("head={}", r.head()),
            ],
            1,
            "",
            "gh: Pull request Pull request is in clean status\n",
        );

        let (code, said) = r.save();

        assert_eq!(code, 0, "{said}");
        assert!(
            was_asked(&enable),
            "the merge was not asked for at the pushed commit"
        );
        assert!(said.contains("#12 is not set to auto-merge"), "{said}");
        assert!(
            said.contains("It stays open for a person to merge"),
            "{said}"
        );
        let line = r.last();
        assert_eq!(line["outcome"], "pr-open");
        assert!(line["detail"].as_str().unwrap().contains("clean status"));
    }

    #[test]
    fn a_pr_a_person_opened_from_the_branch_is_never_rewritten_nor_set_to_merge() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        // pr-merge, on a branch the operator already opened a PR from by hand. Nothing but the
        // lookup is written down: a PATCH, or any auto-merge question, would fail the save.
        let host = "repo-theirs.test";
        let scene = Scene::new(host);
        let r = repo(host, "[repos.widget]\nmode = \"pr-merge\"\n");
        support::git(&r.clone, &["checkout", "-q", "-b", "feature/z"]);
        r.commit("d.md", "their change");
        let looked = lookup(
            &scene,
            "feature/z",
            &format!(
                r#"[{{"number": 8, "html_url": "https://{host}/acme/widget/pull/8", "body": "My own words."}}]"#
            ),
        );

        let (code, said) = r.save();

        assert_eq!(code, 0, "{said}");
        assert!(was_asked(&looked));
        assert_eq!(
            r.remote("feature/z"),
            r.head(),
            "the branch is still pushed"
        );
        assert!(
            said.contains("already has a pull request charter did not open, #8"),
            "{said}"
        );
        assert!(!said.contains("auto-merge"), "{said}");
        let line = r.last();
        assert_eq!(line["pr"], format!("https://{host}/acme/widget/pull/8"));
    }

    const SETTINGS: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){autoMergeAllowed rebaseMergeAllowed mergeCommitAllowed squashMergeAllowed pullRequest(number:$number){id}}}";
    const ENABLE: &str = "mutation($id:ID!,$method:PullRequestMergeMethod!,$head:GitObjectID!){enablePullRequestAutoMerge(input:{pullRequestId:$id,mergeMethod:$method,expectedHeadOid:$head}){clientMutationId}}";
}
