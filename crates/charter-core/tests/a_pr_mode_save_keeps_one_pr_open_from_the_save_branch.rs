//! The plane's `pr` and `pr-merge` modes (charter-app#298, ADR 0051): a save commits on the
//! target branch, pushes it to this machine's one rolling save branch, and keeps one pull
//! request open from there into the target branch — and once that PR has merged, the local
//! target branch is moved onto the remote's, keeping newer work.
//!
//! Real git against a local bare remote, and a stand-in `gh` that answers only what a test
//! wrote down (`support/forge_cli.rs`), so nothing here reaches a network. The remote is
//! reached the way `planegit`'s own tests reach one: `origin` in the SSH form and an
//! `url.<file>.insteadOf` keyed on the HTTPS base, in the plane's LOCAL config, so the URL
//! charter builds is the one git maps onto the bare repository.
//!
//! Each test has a host of its own, declared as a `[[forge]]` in the plane's `charter.toml`,
//! because the stand-in keeps each host's questions in a directory of their own.

mod support;

use std::path::{Path, PathBuf};

use charter_core::planegit::{self, Request, Stage, Trigger};
use charter_core::repocmd::Say;
use support::forge_cli::{Scene, in_a_child, in_child, was_asked};

#[test]
fn every_pr_mode_save_is_run_against_a_stand_in_forge() {
    charter_core::unsteered!();
    in_a_child("pr_saves::", "bin");
}

mod pr_saves {
    use super::*;

    const SAVE: &str = "charter/save/test";

    struct Plane {
        _dir: tempfile::TempDir,
        root: PathBuf,
        bare: PathBuf,
        scene: Scene,
    }

    /// git for the test's own setup.
    fn git(dir: &Path, args: &[&str]) -> String {
        String::from_utf8_lossy(&support::git(dir, args).stdout)
            .trim()
            .to_string()
    }

    /// A plane on `release`, saved in `mode`, whose origin is a bare repository standing in
    /// for a GitHub at `host`.
    fn plane(host: &str, mode: &str) -> Plane {
        let dir = tempfile::tempdir().unwrap();
        let top = std::fs::canonicalize(dir.path()).unwrap();
        let root = top.join("plane");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("charter.toml"),
            format!(
                "[[forge]]\nkind = \"github\"\nhost = \"{host}\"\nowner = \"acme\"\n\n\
                 [plane]\nmode = \"{mode}\"\nbranch = \"release\"\nsave_branch = \"{SAVE}\"\n"
            ),
        )
        .unwrap();
        std::fs::write(root.join(".gitignore"), ".charter/\n").unwrap();
        std::fs::write(root.join("README.md"), "one\n").unwrap();
        git(&root, &["init", "-q", "-b", "release", "."]);
        // The product's own git reads the repo's config, and the child's HOME has none.
        git(&root, &["config", "user.name", "Fixture"]);
        git(&root, &["config", "user.email", "fixture@example.invalid"]);
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-q", "-m", "one"]);
        let bare = top.join("forge/acme/plane.git");
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "-q", "--bare", "-b", "release", "."]);
        git(
            &root,
            &[
                "remote",
                "add",
                "origin",
                &format!("git@{host}:acme/plane.git"),
            ],
        );
        git(
            &root,
            &[
                "config",
                &format!("url.file://{}/.insteadOf", top.join("forge/acme").display()),
                &format!("https://{host}/acme/"),
            ],
        );
        git(
            &root,
            &[
                "push",
                "-q",
                &bare.display().to_string(),
                "HEAD:refs/heads/release",
            ],
        );
        git(
            &root,
            &["update-ref", "refs/remotes/origin/release", "HEAD"],
        );
        Plane {
            _dir: dir,
            root,
            bare,
            scene: Scene::new(host),
        }
    }

    impl Plane {
        fn save_as(&self, message: Option<&str>, trigger: Trigger) -> (u8, String) {
            let mut said = String::new();
            let mut say = |line: Say| {
                said.push_str(&line.to_string());
                said.push('\n');
            };
            let code = planegit::save_as(
                &Request {
                    root: &self.root,
                    message,
                    sign: false,
                    no_push: false,
                    cwd: &self.root,
                },
                trigger,
                &mut say,
            );
            (code, said)
        }

        fn save(&self, file: &str, message: &str) -> (u8, String) {
            std::fs::write(self.root.join(file), message).unwrap();
            self.save_as(Some(message), Trigger::Manual)
        }

        /// A commit in the remote's `release` made by somebody else, through a clone of it.
        fn theirs(&self) -> PathBuf {
            let theirs = self.root.parent().unwrap().join("theirs");
            if !theirs.exists() {
                git(
                    self.root.parent().unwrap(),
                    &[
                        "clone",
                        "-q",
                        &self.bare.display().to_string(),
                        &theirs.display().to_string(),
                    ],
                );
                git(&theirs, &["config", "user.name", "Other"]);
                git(&theirs, &["config", "user.email", "other@example.invalid"]);
            }
            git(&theirs, &["fetch", "-q", "origin"]);
            git(&theirs, &["checkout", "-q", "release"]);
            git(&theirs, &["reset", "-q", "--hard", "origin/release"]);
            theirs
        }

        fn their_commit(&self, file: &str, text: &str) {
            let theirs = self.theirs();
            std::fs::write(theirs.join(file), text).unwrap();
            git(&theirs, &["add", "-A"]);
            git(&theirs, &["commit", "-q", "-m", file]);
            git(&theirs, &["push", "-q", "origin", "release"]);
        }

        fn remote(&self, branch: &str) -> String {
            support::git(&self.bare, &["rev-parse", "--verify", "-q", branch])
                .stdout
                .iter()
                .map(|b| *b as char)
                .collect::<String>()
                .trim()
                .to_string()
        }

        fn head(&self) -> String {
            git(&self.root, &["rev-parse", "HEAD"])
        }

        fn lookup(&self, open: Option<u64>) -> PathBuf {
            let out = match open {
                Some(n) => format!(r#"[{{"number": {n}, "html_url": "{}"}}]"#, self.url(n)),
                None => "[]".to_string(),
            };
            self.scene.gh_api(
                "repos/acme/plane/pulls?state=open&head=acme:charter%2Fsave%2Ftest&base=release&per_page=1",
                0,
                &out,
                "",
            )
        }

        fn url(&self, number: u64) -> String {
            format!("https://{}/acme/plane/pull/{number}", self.scene.host)
        }

        fn created(&self, number: u64, subjects: &[&str]) -> PathBuf {
            let host = self.scene.host.clone();
            self.scene.answers(
                "gh",
                &[
                    "api",
                    "--hostname",
                    &host,
                    "-X",
                    "POST",
                    "repos/acme/plane/pulls",
                    "-f",
                    &format!("head={SAVE}"),
                    "-f",
                    "base=release",
                    "-f",
                    &format!("title={}", title(subjects)),
                    "-f",
                    &format!("body={}", body(subjects)),
                ],
                0,
                &format!(
                    r#"{{"number": {number}, "html_url": "{}"}}"#,
                    self.url(number)
                ),
                "",
            )
        }

        fn updated(&self, number: u64, subjects: &[&str]) -> PathBuf {
            let host = self.scene.host.clone();
            self.scene.answers(
                "gh",
                &[
                    "api",
                    "--hostname",
                    &host,
                    "-X",
                    "PATCH",
                    &format!("repos/acme/plane/pulls/{number}"),
                    "-f",
                    &format!("title={}", title(subjects)),
                    "-f",
                    &format!("body={}", body(subjects)),
                ],
                0,
                &format!(
                    r#"{{"number": {number}, "html_url": "{}"}}"#,
                    self.url(number)
                ),
                "",
            )
        }

        /// PR `number` is open, or closed without merging.
        fn state(&self, number: u64, open: bool) -> PathBuf {
            let state = if open { "open" } else { "closed" };
            self.scene.gh_api(
                &format!("repos/acme/plane/pulls/{number}"),
                0,
                &format!(r#"{{"state": "{state}", "merged": false, "merge_commit_sha": null}}"#),
                "",
            )
        }

        /// PR `number` merged, as the commit `at` on the target branch.
        fn merged(&self, number: u64, at: &str) -> PathBuf {
            self.scene.gh_api(
                &format!("repos/acme/plane/pulls/{number}"),
                0,
                &format!(r#"{{"state": "closed", "merged": true, "merge_commit_sha": "{at}"}}"#),
                "",
            )
        }

        /// A save of `file` that opens PR `number` from the save branch.
        fn opened(&self, file: &str, number: u64) -> String {
            self.lookup(None);
            let created = self.created(number, &[file]);
            let (code, said) = self.save(file, file);
            assert_eq!(code, 0, "{said}");
            assert!(was_asked(&created), "{said}");
            said
        }

        fn standing(&self) -> planegit::Standing {
            planegit::standing(&self.root)
        }

        fn last_journal(&self) -> serde_json::Value {
            planegit::journal(&self.root).pop().expect("a journal line")
        }
    }

    fn title(subjects: &[&str]) -> String {
        match subjects {
            [one] => (*one).to_string(),
            _ => format!(
                "{} saves from {}",
                subjects.len(),
                charter_core::dispatch::host()
            ),
        }
    }

    fn body(subjects: &[&str]) -> String {
        let mut out = format!(
            "Saved by charter on {}. Every save from this machine pushes to {SAVE} and updates \
             this pull request.\n",
            charter_core::dispatch::host()
        );
        for subject in subjects {
            out.push_str(&format!("\n- {subject}"));
        }
        out
    }

    #[test]
    fn a_pr_save_pushes_to_the_save_branch_and_opens_one_pr_into_the_target_branch() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("pr-open.test", "pr");
        let release = plane.remote("release");

        let said = plane.opened("work.md", 12);

        assert_eq!(plane.remote(SAVE), plane.head(), "{said}");
        assert_eq!(
            plane.remote("release"),
            release,
            "the target is never pushed to"
        );
        assert!(said.contains(&plane.url(12)), "{said}");
        assert!(!said.contains("cannot open the pull request"), "{said}");
        let standing = plane.standing();
        assert_eq!(standing.stage, Stage::PrOpen, "{standing:?}");
        assert_eq!(standing.pr.as_deref(), Some(plane.url(12).as_str()));
        let line = plane.last_journal();
        assert_eq!(line["outcome"], "pr-open");
        assert_eq!(line["pr"], plane.url(12).as_str());
    }

    #[test]
    fn the_next_save_updates_the_same_pr_and_never_opens_a_second() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("pr-update.test", "pr");
        plane.opened("one.md", 12);
        let asked = plane.state(12, true);
        plane.lookup(Some(12));
        let updated = plane.updated(12, &["one.md", "two.md"]);

        let (code, said) = plane.save("two.md", "two.md");

        assert_eq!(code, 0, "{said}");
        assert!(was_asked(&updated), "{said}");
        assert!(
            was_asked(&asked),
            "the save asked where its PR stands: {said}"
        );
        assert_eq!(plane.remote(SAVE), plane.head(), "{said}");
        assert_eq!(plane.standing().stage, Stage::PrOpen);
    }

    #[test]
    fn pr_merge_asks_the_forge_to_merge_at_the_commit_it_pushed() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("pr-merge.test", "pr-merge");
        // Committed by an agent with plain git: the save carries it, and its sha is known.
        std::fs::write(plane.root.join("work.md"), "work").unwrap();
        git(&plane.root, &["add", "-A"]);
        git(&plane.root, &["commit", "-q", "-m", "work.md"]);
        let head = plane.head();
        plane.lookup(None);
        plane.created(12, &["work.md"]);
        let host = plane.scene.host.clone();
        plane.scene.answers(
            "gh",
            &[
                "api",
                "graphql",
                "--hostname",
                &host,
                "-f",
                &format!("query={SETTINGS}"),
                "-f",
                "owner=acme",
                "-f",
                "name=plane",
                "-F",
                "number=12",
            ],
            0,
            r#"{"data": {"repository": {"autoMergeAllowed": true, "rebaseMergeAllowed": true,
                "mergeCommitAllowed": true, "squashMergeAllowed": true,
                "pullRequest": {"id": "PR_12"}}}}"#,
            "",
        );
        let enabled = plane.scene.answers(
            "gh",
            &[
                "api",
                "graphql",
                "--hostname",
                &host,
                "-f",
                &format!("query={ENABLE}"),
                "-f",
                "id=PR_12",
                "-f",
                "method=REBASE",
                "-f",
                &format!("head={head}"),
            ],
            1,
            "",
            "GraphQL: Pull request Pull request is in clean status (enablePullRequestAutoMerge)",
        );

        let (code, said) = plane.save_as(None, Trigger::Manual);

        assert_eq!(
            code, 0,
            "a merge the forge would not queue is not a failure: {said}"
        );
        assert!(was_asked(&enabled), "{said}");
        assert!(
            said.contains("clean status"),
            "said in the forge's words: {said}"
        );
        assert_eq!(plane.remote(SAVE), head);
        assert_eq!(plane.standing().stage, Stage::PrOpen);
    }

    const SETTINGS: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){autoMergeAllowed rebaseMergeAllowed mergeCommitAllowed squashMergeAllowed pullRequest(number:$number){id}}}";
    const ENABLE: &str = "mutation($id:ID!,$method:PullRequestMergeMethod!,$head:GitObjectID!){enablePullRequestAutoMerge(input:{pullRequestId:$id,mergeMethod:$method,expectedHeadOid:$head}){clientMutationId}}";

    #[test]
    fn a_commit_left_unpushed_is_carried_to_the_pr_by_the_next_save_with_nothing_new() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("pr-launch.test", "pr");
        std::fs::write(plane.root.join("work.md"), "work").unwrap();
        git(&plane.root, &["add", "-A"]);
        git(&plane.root, &["commit", "-q", "-m", "work.md"]);
        let standing = plane.standing();
        assert_eq!(standing.stage, Stage::Committed, "{standing:?}");
        assert!(charter_core::autosave::worth_saving(&standing));
        plane.lookup(None);
        let created = plane.created(12, &["work.md"]);

        let (code, said) = plane.save_as(None, Trigger::Launch);

        assert_eq!(code, 0, "{said}");
        assert!(was_asked(&created), "{said}");
        assert_eq!(plane.remote(SAVE), plane.head());
        assert_eq!(plane.standing().stage, Stage::PrOpen);
    }

    #[test]
    fn quitting_in_a_pr_mode_pushes_to_the_save_branch_and_never_to_the_target() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("pr-quit.test", "pr");
        let release = plane.remote("release");
        std::fs::write(plane.root.join("work.md"), "work").unwrap();
        plane.lookup(None);
        let created = plane.created(12, &["charter save: 1 file (work.md 1)"]);

        let quit = charter_core::autosave::at_quit(&plane.root, std::time::Duration::from_secs(30));

        assert_eq!(quit, charter_core::autosave::AtQuit::Pushed);
        assert!(was_asked(&created));
        assert_eq!(plane.remote(SAVE), plane.head());
        assert_eq!(plane.remote("release"), release);
    }

    /// Somebody else's commit on the remote's `release`, then the save branch merged into it
    /// by `how`, as the forge would do it. Answers the commit the merge made on `release`, which
    /// is what the forge names as the PR's merge commit.
    fn merge(plane: &Plane, how: &str) -> String {
        plane.their_commit("theirs.md", "theirs");
        let theirs = plane.theirs();
        let save = format!("origin/{SAVE}");
        match how {
            "merge" => {
                git(
                    &theirs,
                    &[
                        "merge",
                        "-q",
                        "--no-ff",
                        "-m",
                        "Merge pull request #12",
                        &save,
                    ],
                );
            }
            "rebase" => {
                git(&theirs, &["checkout", "-q", "-b", "rebased", &save]);
                git(&theirs, &["rebase", "-q", "release"]);
                git(&theirs, &["checkout", "-q", "release"]);
                git(&theirs, &["merge", "-q", "--ff-only", "rebased"]);
            }
            "squash" => {
                git(&theirs, &["merge", "-q", "--squash", &save]);
                git(&theirs, &["commit", "-q", "-m", "Squashed (#12)"]);
            }
            // A squash whose merger changed one of the PR's files on the way in.
            "squash-edited" => {
                git(&theirs, &["merge", "-q", "--squash", &save]);
                std::fs::write(theirs.join("work.md"), "edited while merging").unwrap();
                git(&theirs, &["add", "-A"]);
                git(&theirs, &["commit", "-q", "-m", "Squashed (#12)"]);
            }
            other => panic!("no merge called {other}"),
        }
        git(&theirs, &["push", "-q", "origin", "release"]);
        git(&theirs, &["rev-parse", "HEAD"])
    }

    fn merged_then_fetched_keeps_newer_work(host: &str, how: &str) {
        let plane = plane(host, "pr");
        plane.opened("work.md", 12);
        let at = merge(&plane, how);
        // Newer work, not yet saved: it must be kept.
        std::fs::write(plane.root.join("later.md"), "later").unwrap();
        let asked = plane.merged(12, &at);

        let incoming = planegit::fetch(&plane.root, true).expect("the fetch");

        assert!(was_asked(&asked), "{incoming:?}");
        assert_eq!(
            plane.head(),
            plane.remote("release"),
            "{how}: moved onto the remote"
        );
        assert_eq!(
            std::fs::read_to_string(plane.root.join("later.md")).unwrap(),
            "later",
            "{how}: newer work kept"
        );
        assert_eq!(
            std::fs::read_to_string(plane.root.join("work.md")).unwrap(),
            "work.md",
            "{how}"
        );
        let standing = plane.standing();
        assert_eq!(standing.stage, Stage::Changed, "{how}: {standing:?}");
        assert_eq!(standing.changed, vec!["later.md".to_string()]);
        assert_eq!(standing.pr, None, "{how}");
        assert_eq!(standing.ahead, Some(0), "{how}");
    }

    #[test]
    fn after_a_merge_commit_the_plane_moves_onto_the_target_keeping_newer_work() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        merged_then_fetched_keeps_newer_work("merged-merge.test", "merge");
    }

    #[test]
    fn after_a_rebase_merge_the_plane_moves_onto_the_target_keeping_newer_work() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        merged_then_fetched_keeps_newer_work("merged-rebase.test", "rebase");
    }

    #[test]
    fn after_a_squash_merge_the_plane_moves_onto_the_target_keeping_newer_work() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        merged_then_fetched_keeps_newer_work("merged-squash.test", "squash");
    }

    #[test]
    fn a_save_after_a_squash_merge_replays_the_newer_commit_and_opens_a_fresh_pr() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("squash-then-save.test", "pr");
        plane.opened("work.md", 12);
        let at = merge(&plane, "squash");
        plane.merged(12, &at);
        plane.lookup(None);
        let created = plane.created(13, &["later.md"]);

        let (code, said) = plane.save("later.md", "later.md");

        assert_eq!(code, 0, "{said}");
        assert!(was_asked(&created), "{said}");
        let remote = plane.remote("release");
        assert_eq!(
            git(&plane.root, &["rev-parse", "HEAD~1"]),
            remote,
            "the newer commit sits on the remote's release: {said}"
        );
        assert_eq!(git(&plane.root, &["log", "-1", "--format=%s"]), "later.md");
        // The save branch was moved to a commit that does not descend from its old tip, which
        // only the lease on that one charter-owned branch allows.
        assert_eq!(plane.remote(SAVE), plane.head());
        assert_eq!(plane.standing().pr, Some(plane.url(13)));
    }

    #[test]
    fn a_pr_closed_without_merging_blocks_the_plane_and_the_next_save_opens_a_new_one() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("closed.test", "pr");
        plane.opened("work.md", 12);
        let head = plane.head();
        let release = plane.remote("release");
        plane.state(12, false);

        planegit::fetch(&plane.root, true).expect("the fetch");

        let standing = plane.standing();
        assert_eq!(standing.stage, Stage::Blocked, "{standing:?}");
        assert!(
            standing
                .blocked
                .as_deref()
                .is_some_and(|why| why.contains("closed without merging")),
            "{standing:?}"
        );
        assert_eq!(
            plane.head(),
            head,
            "nothing of this machine's is thrown away"
        );
        assert_eq!(plane.remote("release"), release);

        // A person saving again is the way out: a new PR for the same commits.
        plane.lookup(None);
        let created = plane.created(13, &["work.md"]);
        let (code, said) = plane.save_as(None, Trigger::Manual);
        assert_eq!(code, 0, "{said}");
        assert!(was_asked(&created), "{said}");
        assert_eq!(plane.standing().stage, Stage::PrOpen);
        assert_eq!(plane.standing().pr, Some(plane.url(13)));
    }

    #[test]
    fn a_merge_commit_that_does_not_hold_what_was_pushed_blocks_the_plane() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("diverged.test", "pr");
        plane.opened("work.md", 12);
        let head = plane.head();
        let at = merge(&plane, "squash-edited");
        plane.merged(12, &at);

        planegit::fetch(&plane.root, true).expect("the fetch");

        let standing = plane.standing();
        assert_eq!(standing.stage, Stage::Blocked, "{standing:?}");
        assert!(
            standing
                .blocked
                .as_deref()
                .is_some_and(|why| why.contains("work.md")),
            "{standing:?}"
        );
        assert_eq!(plane.head(), head, "the local branch is left where it was");
    }

    #[test]
    fn a_save_branch_somebody_else_pushed_to_is_never_overwritten() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("lease.test", "pr");
        plane.opened("one.md", 12);
        // Somebody else's commit on this machine's save branch.
        let theirs = plane.theirs();
        git(&theirs, &["fetch", "-q", "origin"]);
        git(
            &theirs,
            &[
                "checkout",
                "-q",
                "-B",
                "stranger",
                &format!("origin/{SAVE}"),
            ],
        );
        std::fs::write(theirs.join("stranger.md"), "stranger").unwrap();
        git(&theirs, &["add", "-A"]);
        git(&theirs, &["commit", "-q", "-m", "stranger"]);
        git(
            &theirs,
            &["push", "-q", "origin", &format!("stranger:{SAVE}")],
        );
        let stranger = plane.remote(SAVE);
        plane.state(12, true);

        let (code, said) = plane.save("two.md", "two.md");

        assert_eq!(code, 0, "{said}");
        assert_eq!(
            plane.remote(SAVE),
            stranger,
            "their commit is still there: {said}"
        );
        assert_eq!(plane.standing().stage, Stage::Blocked, "{said}");

        // Once the stranger's branch is gone from the remote, the next save pushes again.
        git(&theirs, &["push", "-q", "origin", &format!(":{SAVE}")]);
        plane.state(12, true);
        plane.lookup(Some(12));
        let updated = plane.updated(12, &["one.md", "two.md"]);
        let (code, said) = plane.save_as(None, Trigger::Manual);
        assert_eq!(code, 0, "{said}");
        assert!(was_asked(&updated), "{said}");
        assert_eq!(plane.remote(SAVE), plane.head(), "{said}");
        assert_eq!(plane.standing().stage, Stage::PrOpen, "{said}");
    }

    #[test]
    fn a_save_branch_that_is_the_target_branch_is_refused() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("same-branch.test", "pr");
        let toml = std::fs::read_to_string(plane.root.join("charter.toml"))
            .unwrap()
            .replace(SAVE, "release");
        std::fs::write(plane.root.join("charter.toml"), toml).unwrap();
        git(&plane.root, &["commit", "-q", "-am", "settings"]);
        let release = plane.remote("release");

        let (code, said) = plane.save("work.md", "work.md");

        assert_eq!(code, 0, "{said}");
        assert_eq!(plane.remote("release"), release, "{said}");
        assert!(said.contains("save_branch"), "{said}");
        assert_eq!(plane.last_journal()["outcome"], "blocked");
    }

    #[test]
    fn a_later_edit_to_a_path_the_pr_touched_does_not_stop_a_squash_merge_from_settling() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("later-edit.test", "pr");
        plane.opened("work.md", 12);
        let at = merge(&plane, "squash");
        // Somebody else edits the same file after the merge, as MEMORY.md is edited all day.
        plane.their_commit("work.md", "rewritten by someone else");
        plane.merged(12, &at);

        planegit::fetch(&plane.root, true).expect("the fetch");

        let standing = plane.standing();
        assert_ne!(standing.stage, Stage::Blocked, "{standing:?}");
        assert_eq!(plane.head(), plane.remote("release"), "{standing:?}");
        assert_eq!(
            std::fs::read_to_string(plane.root.join("work.md")).unwrap(),
            "rewritten by someone else"
        );
    }

    #[test]
    fn a_file_in_the_way_of_the_move_waits_for_the_next_look_rather_than_blocking() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("in-the-way.test", "pr");
        plane.opened("work.md", 12);
        let head = plane.head();
        let at = merge(&plane, "squash");
        // An untracked file the remote's branch now has: `reset --keep` will not overwrite it.
        std::fs::write(plane.root.join("theirs.md"), "mine").unwrap();
        plane.merged(12, &at);

        planegit::fetch(&plane.root, true).expect("the fetch");

        let standing = plane.standing();
        assert_ne!(standing.stage, Stage::Blocked, "{standing:?}");
        assert_eq!(plane.head(), head, "nothing moved");
        assert_eq!(
            std::fs::read_to_string(plane.root.join("theirs.md")).unwrap(),
            "mine"
        );

        // Out of the way, the next look moves the plane.
        std::fs::remove_file(plane.root.join("theirs.md")).unwrap();
        let asked = plane.merged(12, &at);
        planegit::fetch(&plane.root, true).expect("the fetch");
        assert!(was_asked(&asked));
        assert_eq!(plane.head(), plane.remote("release"));
    }

    #[test]
    fn a_pr_that_merged_after_a_failed_save_is_settled_and_never_opened_again() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("no-duplicate.test", "pr");
        plane.opened("one.md", 12);
        // The next save pushes, and the forge then fails to answer.
        plane.state(12, true);
        plane.scene.gh_api(
            "repos/acme/plane/pulls?state=open&head=acme:charter%2Fsave%2Ftest&base=release&per_page=1",
            1,
            "",
            "HTTP 502: Bad Gateway",
        );
        let (code, said) = plane.save("two.md", "two.md");
        assert_eq!(code, 0, "{said}");
        assert!(said.contains("HTTP 502"), "{said}");
        // PR 12 merges, carrying both commits.
        let at = merge(&plane, "squash");
        plane.merged(12, &at);
        plane.lookup(None);
        // Only the commit made since: nothing PR 12 already carried is proposed again.
        let created = plane.created(13, &["three.md"]);

        let (code, said) = plane.save("three.md", "three.md");

        assert_eq!(code, 0, "{said}");
        assert!(was_asked(&created), "{said}");
        assert_eq!(
            git(&plane.root, &["rev-parse", "HEAD~1"]),
            plane.remote("release")
        );
    }

    #[test]
    fn a_pr_whose_state_the_forge_cannot_give_stops_the_save_before_it_pushes() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("state-fails.test", "pr");
        plane.opened("one.md", 12);
        let pushed = plane.remote(SAVE);
        plane
            .scene
            .gh_api("repos/acme/plane/pulls/12", 1, "", "HTTP 502: Bad Gateway");

        let (code, said) = plane.save("two.md", "two.md");

        assert_eq!(code, 0, "{said}");
        assert!(said.contains("HTTP 502"), "{said}");
        assert_eq!(plane.remote(SAVE), pushed, "nothing pushed: {said}");
        assert_eq!(plane.last_journal()["outcome"], "failed");
        assert_ne!(plane.standing().stage, Stage::Blocked);
    }

    #[test]
    fn a_save_with_nothing_to_commit_settles_a_merged_pr() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("clean-settle.test", "pr");
        plane.opened("work.md", 12);
        let at = merge(&plane, "squash");
        plane.merged(12, &at);

        let (code, said) = plane.save_as(None, Trigger::Cli);

        assert_eq!(code, 0, "{said}");
        assert_eq!(plane.head(), plane.remote("release"), "{said}");
        assert_eq!(plane.standing().stage, Stage::Saved, "{said}");
        assert_eq!(plane.standing().pr, None);
    }

    #[test]
    fn a_second_machine_with_the_same_save_branch_never_overwrites_the_firsts() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let plane = plane("same-host.test", "pr");
        plane.opened("mine.md", 12);
        let first = plane.remote(SAVE);
        // Another clone of the same plane, on a machine with the same name.
        let other = plane.root.parent().unwrap().join("other");
        git(
            plane.root.parent().unwrap(),
            &[
                "clone",
                "-q",
                &plane.bare.display().to_string(),
                &other.display().to_string(),
            ],
        );
        git(&other, &["config", "user.name", "Other"]);
        git(&other, &["config", "user.email", "other@example.invalid"]);
        let host = plane.scene.host.clone();
        git(
            &other,
            &[
                "remote",
                "set-url",
                "origin",
                &format!("git@{host}:acme/plane.git"),
            ],
        );
        git(
            &other,
            &[
                "config",
                &format!(
                    "url.file://{}/.insteadOf",
                    plane.bare.parent().unwrap().display()
                ),
                &format!("https://{host}/acme/"),
            ],
        );
        std::fs::write(other.join("theirs.md"), "theirs").unwrap();
        let mut said = String::new();
        let code = planegit::save_as(
            &Request {
                root: &other,
                message: Some("theirs.md"),
                sign: false,
                no_push: false,
                cwd: &other,
            },
            Trigger::Manual,
            &mut |line: Say| said.push_str(&format!("{line}\n")),
        );

        assert_eq!(code, 0, "{said}");
        assert_eq!(
            plane.remote(SAVE),
            first,
            "the first machine's commits stay: {said}"
        );
        assert_eq!(planegit::standing(&other).stage, Stage::Blocked, "{said}");

        // Fixed by hand, as the block says: a save branch of this clone's own, in its local
        // file. The block is gone at once, before any save runs.
        std::fs::write(other.join(".gitignore"), ".charter/\ncharter.local.toml\n").unwrap();
        git(&other, &["commit", "-q", "-am", "ignore the local file"]);
        std::fs::write(
            other.join("charter.local.toml"),
            "[plane]\nsave_branch = \"charter/save/other\"\n",
        )
        .unwrap();
        assert_ne!(planegit::standing(&other).stage, Stage::Blocked);
    }
}
