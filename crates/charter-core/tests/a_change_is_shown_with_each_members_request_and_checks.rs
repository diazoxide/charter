//! `charter change show`'s forge half: each member's request, whether it merged, and the
//! checks at its exact head (#469, ADR 0060).
//!
//! Every question goes to a stand-in `gh` or `glab` that answers only what a test wrote down,
//! exactly once (`support/forge_cli.rs`). Nothing reaches a network, and a question nobody wrote
//! down, `gh pr checks` or a `mergeStateStatus` query among them, exits 99 and turns the member
//! it was asked for into "could not ask" or `UNKNOWN`, which each test would see.

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use charter_core::change::cmd;
use charter_core::change::observe::{Observation, observe};
use charter_core::change::{Member, Record, store};
use charter_core::forge::checks::Ci;
use charter_core::forge::pr::State;
use charter_core::repocmd::Say;
use support::forge_cli::{Scene, in_a_child, in_child};

const HEAD: &str = "9f3a1c2d4e5f60718293a4b5c6d7e8f901234567";

#[test]
fn every_change_show_question_is_asked_of_a_stand_in_cli() {
    charter_core::unsteered!();
    in_a_child("shown::", "bin");
}

#[test]
fn nothing_on_the_show_path_reads_a_roll_up_that_turns_silence_green() {
    charter_core::unsteered!();
    for (file, source) in [
        (
            "change/observe.rs",
            include_str!("../src/change/observe.rs"),
        ),
        ("forge/checks.rs", include_str!("../src/forge/checks.rs")),
        ("forge/pr.rs", include_str!("../src/forge/pr.rs")),
        ("change/cmd.rs", include_str!("../src/change/cmd.rs")),
    ] {
        for (forbidden, label) in [
            ("\"pr\", \"checks\"", "gh pr checks"),
            ("statusCheckRollup", "the status check roll-up"),
            ("glstate::", "the status line's forge cache"),
            ("ci_status(", "the status line's CI word"),
        ] {
            assert!(!source.contains(forbidden), "{file} reads {label}");
        }
        let code: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect();
        assert!(
            !code.contains("mergeStateStatus"),
            "{file} reads mergeStateStatus"
        );
    }
}

mod shown {
    use super::*;

    /// A plane whose `charter.toml` declares the scene's host, with clones in `alpha` whose
    /// `origin` is on it, and one change over them.
    struct World {
        _tmp: tempfile::TempDir,
        plane: PathBuf,
    }

    fn git(dir: &Path, args: &[&str]) {
        let out = charter_core::forklock::output(
            Command::new("git")
                .args(args)
                .current_dir(dir)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1"),
        )
        .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }

    fn world(scene: &Scene, kind: &str, members: &[(&str, &[&str])]) -> World {
        let tmp = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(tmp.path()).unwrap();
        std::fs::write(
            plane.join("charter.toml"),
            format!(
                "schema = 1\n\n[[forge]]\nkind = \"{kind}\"\nhost = \"{}\"\n",
                scene.host
            ),
        )
        .unwrap();
        let mut record = Record::new("api-2", "bump", "t", "2026-09-26T00:00:00+00:00");
        for (repo, needs) in members {
            let clone = plane.join("workspaces/alpha").join(repo);
            std::fs::create_dir_all(&clone).unwrap();
            git(&clone, &["init", "-q", "-b", "main", "."]);
            git(
                &clone,
                &[
                    "remote",
                    "add",
                    "origin",
                    &format!("https://{}/acme/{repo}.git", scene.host),
                ],
            );
            record.members.push(Member {
                repo: (*repo).into(),
                branch: "change/api-2".into(),
                needs: needs.iter().map(|n| (*n).to_string()).collect(),
            });
        }
        store::write(&plane, "alpha", &record).unwrap();
        World { _tmp: tmp, plane }
    }

    fn look(world: &World) -> Observation {
        let record = store::read(&world.plane, "alpha", "api-2").unwrap();
        observe(&world.plane, "alpha", &record, chrono::Utc::now())
    }

    fn pulls(scene: &Scene, repo: &str, out: &str) {
        scene.gh_api(
            &format!("repos/acme/{repo}/pulls?state=all&head=acme:change%2Fapi-2&per_page=1"),
            0,
            out,
            "",
        );
    }

    fn open_pr(repo: &str, n: u64) -> String {
        format!(
            r#"[{{"number": {n}, "html_url": "https://x/pull/{n}", "state": "open", "head": {{"sha": "{HEAD}", "ref": "change/api-2", "repo": {{"full_name": "acme/{repo}"}}}}}}]"#
        )
    }

    fn checks(scene: &Scene, repo: &str, runs: &str, statuses: &str) {
        let base = format!("repos/acme/{repo}/commits/{HEAD}");
        scene.gh_api(&format!("{base}/check-runs?per_page=100"), 0, runs, "");
        scene.gh_api(&format!("{base}/status?per_page=100"), 0, statuses, "");
    }

    const NO_STATUSES: &str = r#"{"state": "pending", "total_count": 0, "statuses": []}"#;

    #[test]
    fn an_open_request_shows_its_number_its_head_and_passed_checks() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("show-passed.test");
        let world = world(&scene, "github", &[("svc", &[])]);
        pulls(&scene, "svc", &open_pr("svc", 601));
        checks(
            &scene,
            "svc",
            r#"{"total_count": 2, "check_runs": [{"status": "completed", "conclusion": "success"}, {"status": "completed", "conclusion": "skipped"}]}"#,
            NO_STATUSES,
        );
        let seen = look(&world);
        let m = &seen.members[0];
        let req = m.request.clone().unwrap().unwrap();
        assert_eq!(
            (req.number, req.state, req.head.as_str()),
            (601, State::Open, HEAD)
        );
        let checks = m.checks.clone().unwrap();
        assert_eq!((checks.total, checks.ci), (Some(2), Ci::Passed));
    }

    #[test]
    fn zero_checks_at_the_head_is_not_run_and_the_row_never_says_passed() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("show-not-run.test");
        let world = world(&scene, "github", &[("svc", &[])]);
        pulls(&scene, "svc", &open_pr("svc", 7));
        checks(
            &scene,
            "svc",
            r#"{"total_count": 0, "check_runs": []}"#,
            NO_STATUSES,
        );
        let mut lines = Vec::new();
        let code = cmd::show(
            &world.plane,
            "alpha",
            "api-2",
            chrono::Utc::now(),
            &mut |say| {
                if let Say::Out(line) = say {
                    lines.push(line)
                }
            },
        );
        assert_eq!(code, 0);
        let out = lines.join("\n");
        assert!(
            out.contains("svc  #7 open  head 9f3a1c2  checks NOT RUN"),
            "{out}"
        );
        assert!(!out.contains("PASSED"), "{out}");
    }

    #[test]
    fn a_status_only_ci_is_seen_through_the_commit_statuses() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("show-statuses.test");
        let world = world(&scene, "github", &[("svc", &[])]);
        pulls(&scene, "svc", &open_pr("svc", 8));
        checks(
            &scene,
            "svc",
            r#"{"total_count": 0, "check_runs": []}"#,
            r#"{"state": "failure", "total_count": 1, "statuses": [{"state": "failure"}]}"#,
        );
        let checks = look(&world).members[0].checks.clone().unwrap();
        assert_eq!((checks.total, checks.ci), (Some(1), Ci::Failed));
    }

    #[test]
    fn checks_charter_could_not_read_are_unknown_never_not_run() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("show-unknown.test");
        let world = world(&scene, "github", &[("svc", &[])]);
        pulls(&scene, "svc", &open_pr("svc", 9));
        scene.gh_api(
            &format!("repos/acme/svc/commits/{HEAD}/check-runs?per_page=100"),
            1,
            "",
            "HTTP 403: rate limited",
        );
        let checks = look(&world).members[0].checks.clone().unwrap();
        assert_eq!((checks.total, checks.ci), (None, Ci::Unknown));
        assert!(checks.why.unwrap().contains("rate limited"));
    }

    #[test]
    fn a_page_that_does_not_hold_every_run_is_unknown() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("show-paged.test");
        let world = world(&scene, "github", &[("svc", &[])]);
        pulls(&scene, "svc", &open_pr("svc", 10));
        checks(
            &scene,
            "svc",
            r#"{"total_count": 101, "check_runs": [{"status": "completed", "conclusion": "success"}]}"#,
            NO_STATUSES,
        );
        assert_eq!(
            look(&world).members[0].checks.clone().unwrap().ci,
            Ci::Unknown
        );
    }

    #[test]
    fn a_member_with_no_request_says_so_and_its_blocker_is_not_landed() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("show-blocked.test");
        let world = world(&scene, "github", &[("svc", &[]), ("web", &["svc"])]);
        pulls(&scene, "svc", "[]");
        pulls(&scene, "web", "[]");
        let seen = look(&world);
        assert_eq!(seen.members[0].request, Ok(None));
        assert_eq!(seen.members[1].waiting_on, vec!["svc".to_string()]);
        assert_eq!(seen.landed(), (0, 2));
    }

    #[test]
    fn a_merged_blocker_unblocks_its_dependent_and_is_counted_merged() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("show-merged.test");
        let world = world(&scene, "github", &[("svc", &[]), ("web", &["svc"])]);
        pulls(
            &scene,
            "svc",
            &format!(
                r#"[{{"number": 601, "html_url": "u", "state": "closed", "merged_at": "2026-09-26T00:00:00Z", "merge_commit_sha": "e0c9d13", "head": {{"sha": "{HEAD}", "ref": "change/api-2", "repo": {{"full_name": "acme/svc"}}}}}}]"#
            ),
        );
        pulls(&scene, "web", &open_pr("web", 14));
        checks(
            &scene,
            "web",
            r#"{"total_count": 1, "check_runs": [{"status": "in_progress"}]}"#,
            NO_STATUSES,
        );
        let seen = look(&world);
        assert!(seen.members[0].merged());
        assert!(
            seen.members[0].checks.is_none(),
            "a merged request's checks are not read"
        );
        assert!(seen.members[1].waiting_on.is_empty());
        assert_eq!(seen.members[1].checks.clone().unwrap().ci, Ci::Running);
        assert_eq!(seen.landed(), (1, 2));
        let record = store::read(&world.plane, "alpha", "api-2").unwrap();
        let lines = cmd::observed_lines(&record, &seen);
        assert!(lines[0].contains("1 of 2 merged"), "{lines:?}");
        assert!(
            lines.iter().any(|l| l.contains("needs: svc ✓")),
            "{lines:?}"
        );
    }

    #[test]
    fn a_forge_that_cannot_be_asked_costs_only_its_member_and_nothing_is_written_back() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("show-down.test");
        let world = world(&scene, "github", &[("svc", &[])]);
        let before =
            std::fs::read(store::path_for(&world.plane, "alpha", "api-2").unwrap()).unwrap();
        scene.gh_api(
            "repos/acme/svc/pulls?state=all&head=acme:change%2Fapi-2&per_page=1",
            1,
            "",
            "error connecting to show-down.test",
        );
        let seen = look(&world);
        assert!(
            seen.members[0]
                .request
                .clone()
                .unwrap_err()
                .contains("error connecting")
        );
        assert!(seen.members[0].checks.is_none());
        let after =
            std::fs::read(store::path_for(&world.plane, "alpha", "api-2").unwrap()).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn a_gitlab_merge_request_is_read_with_its_own_pipelines_at_the_head() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("show-gitlab.test");
        let world = world(&scene, "gitlab", &[("svc", &[])]);
        scene.glab_api(
            "projects/acme%2Fsvc/merge_requests?source_branch=change%2Fapi-2&state=all&per_page=100",
            0,
            &format!(
                r#"[{{"iid": 3, "web_url": "u", "state": "opened", "sha": "{HEAD}", "source_project_id": 1, "target_project_id": 1}}]"#
            ),
            "",
        );
        scene.glab_api(
            "projects/acme%2Fsvc/merge_requests/3/pipelines?per_page=100",
            0,
            &format!(r#"[{{"id": 2, "sha": "{HEAD}", "status": "success"}}, {{"id": 1, "sha": "{HEAD}", "status": "failed"}}]"#),
            "",
        );
        let checks = look(&world).members[0].checks.clone().unwrap();
        assert_eq!((checks.total, checks.ci), (Some(1), Ci::Passed));
    }

    #[test]
    fn a_gitlab_pipeline_only_at_another_commit_is_unknown_not_not_run() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("show-gitlab-merged-results.test");
        let world = world(&scene, "gitlab", &[("svc", &[])]);
        scene.glab_api(
            "projects/acme%2Fsvc/merge_requests?source_branch=change%2Fapi-2&state=all&per_page=100",
            0,
            &format!(
                r#"[{{"iid": 3, "web_url": "u", "state": "opened", "sha": "{HEAD}", "source_project_id": 1, "target_project_id": 1}}]"#
            ),
            "",
        );
        scene.glab_api(
            "projects/acme%2Fsvc/merge_requests/3/pipelines?per_page=100",
            0,
            r#"[{"id": 2, "sha": "1111111111111111111111111111111111111111", "status": "success"}]"#,
            "",
        );
        assert_eq!(
            look(&world).members[0].checks.clone().unwrap().ci,
            Ci::Unknown
        );
    }

    #[test]
    fn a_pull_request_from_another_branch_is_never_read_as_this_members() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("show-wrong-head.test");
        let world = world(&scene, "github", &[("svc", &[])]);
        pulls(
            &scene,
            "svc",
            &format!(
                r#"[{{"number": 1, "html_url": "u", "state": "open", "head": {{"sha": "{HEAD}", "ref": "main", "repo": {{"full_name": "acme/svc"}}}}}}]"#
            ),
        );
        let seen = look(&world);
        let why = seen.members[0].request.clone().unwrap_err();
        assert!(why.contains("not from this branch"), "{why}");
        assert!(seen.members[0].checks.is_none());
    }
}
