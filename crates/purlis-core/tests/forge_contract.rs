//! The forge contract suite: every method of the forge seam's areas, run once per forge
//! against recorded exchanges (ADR 0070 §7, the recorded run; FG-3), and nightly against a real
//! forge (the live run; FG-4).
//!
//! Each case is written **once**, as a function of the forge, and [`contract!`] instantiates it
//! for GitHub and for GitLab. The two forges' recordings, under `tests/forge_contract/<forge>/`,
//! describe the same scene in each forge's own words, so a case asserts one neutral answer for
//! both. A request a backend sends that nobody recorded fails the case, and so does a recorded
//! exchange nobody asked for.
//!
//! **Where the answers come from.** Each recording names its source. Today they are taken from
//! each forge's own API documentation (GitHub REST `2022-11-28` and its GraphQL schema; the
//! GitLab 19.4 REST docs), not captured from a live forge: no account is signed in where this
//! suite was written. A PR that moves a recording says which contract moved and why (ADR 0046).
//!
//! **The live run** (`live.rs`) is the same cases, through the same seam, over the native
//! transport to a real github.com repo and a real gitlab.com repo the operator provisioned. A
//! case names what it asks about through a [`Scene`] rather than a literal, so the recorded
//! scene and the live fixture both answer it. It is ignored in a plain `cargo test`;
//! `.github/workflows/forge-live.yml` runs it nightly. A case it cannot run is listed in
//! [`NOT_LIVE`] with its reason and ticket, and says so when it is asked, never silently.
//!
//! **Parity is checked, not trusted.** `every_method_of_the_seam_has_a_case_on_both_forges`
//! reads the area traits out of `src/forge/backend.rs` and fails when a method has no case here
//! or a case has no recording for one forge.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use purlis_core::forge::backend::{About, Issues, NewWorkItem, PushProtection, Visibility};
use purlis_core::forge::checks::{Checks, Ci};
use purlis_core::forge::pr::{AutoMerge, MergeAs, MergedAt, Opened, Pr, Request, State};
use purlis_core::forge::recorded::Recorded;
use purlis_core::forge::{
    Caller, Capability, Forge, ForgeBackend, ForgeRef, Kind, Owner, Reach, Support,
};
use purlis_core::work;
use serde_json::{Value, json};

#[path = "forge_contract/live.rs"]
mod live;
#[path = "forge_contract/native.rs"]
mod native;
#[path = "forge_contract/scene.rs"]
mod scene;

use scene::{MERGE, SAVE, SHA, Scene};

/// Every case, by the seam method it covers. The parity test compares this with the traits.
const CASES: [&str; 19] = [
    "owned",
    "about",
    "reachable",
    "top_level",
    "open_or_update",
    "state",
    "body",
    "set_body",
    "by_head",
    "request_auto_merge",
    "lands_through_queue",
    "merge_at",
    "enqueue_at",
    "checks_at",
    "open_on_branch",
    "ci_word",
    "support",
    "create",
    "read",
];

/// The cases the live run does not run, each with why and the ticket that will run it. Each one
/// would merge or queue the fixture's one open request, which the read cases need open; running
/// them needs a request opened afresh for each run.
const NOT_LIVE: [(&str, &str); 4] = [
    (
        "request_auto_merge",
        "auto-merge would land the fixture's open request; it needs a request opened per run (#712)",
    ),
    (
        "merge_at",
        "it merges the fixture's open request; it needs a request opened per run (#712)",
    ),
    (
        "enqueue_at",
        "it needs a merge queue or train on the fixture, and a request opened per run (#712)",
    ),
    (
        "read",
        "it needs a fixture issue with a type, a parent, a child, a blocker, a closing request \
         and a board with a status and an iteration (#742)",
    ),
];

fn recordings() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/forge_contract")
}

/// How a case's requests are answered.
#[derive(Clone, Copy)]
pub enum How {
    /// Through the recorded transport, as a CLI call's reply.
    Recorded,
    /// Through the native transport, over HTTP, against a server that answers the same
    /// recording (`native.rs`), on the forge's own instance: github.com or gitlab.com.
    Native,
    /// As `Native`, on a self-managed instance: a GitHub Enterprise Server or a self-managed
    /// GitLab, at `forge.example.com`.
    SelfManaged,
    /// Through the native transport to a real forge, about the fixture provisioned there
    /// (`live.rs`).
    Live,
}

/// A backend over a case's recording or a live forge, the caller to ask as, the scene to ask
/// about, and the check that every recorded exchange was asked for.
pub struct Over {
    pub backend: Box<dyn ForgeBackend>,
    pub caller: Caller,
    /// The instance the backend is on.
    pub host: String,
    pub scene: Scene,
    check: Box<dyn Fn()>,
    /// Closes every open issue the `create` case opened, by the forge's own listing, and answers
    /// their numbers: on a live forge, so the fixture keeps no backlog. Nothing on a recording.
    sweep: Box<dyn Fn() -> Result<Vec<u64>, String>>,
}

/// Every recorded exchange was asked for.
pub trait Spent {
    fn spent(&self);
}

impl Spent for Recorded {
    fn spent(&self) {
        assert_eq!(self.unspent(), Vec::new(), "recorded and never asked");
    }
}

impl<T: Spent> Spent for Arc<T> {
    fn spent(&self) {
        T::spent(self);
    }
}

impl Spent for Over {
    fn spent(&self) {
        (self.check)();
    }
}

fn spent(recorded: &impl Spent) {
    recorded.spent();
}

/// The text of `kind`'s recording of `case`.
pub fn recording(kind: &str, case: &str) -> String {
    let file = recordings().join(kind).join(format!("{case}.json"));
    std::fs::read_to_string(&file)
        .unwrap_or_else(|e| panic!("the recording {}: {e}", file.display()))
}

/// The self-managed override at `file`, or `None` when there is none. Any other read error
/// fails, so an override that is there is never quietly replaced by the main recording.
fn own_recording(file: &Path) -> Option<String> {
    match std::fs::read_to_string(file) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => panic!("the override {}: {e}", file.display()),
    }
}

/// A forge's backend for `case`, answered as `how` says, or `None`, said aloud, for a case the
/// live run leaves out.
fn over(kind: &str, case: &str, how: How) -> Option<Over> {
    let forge_kind = Kind::parse(kind).unwrap();
    if let How::Live = how {
        if let Some((_, why)) = NOT_LIVE.iter().find(|(c, _)| *c == case) {
            println!("not run live on {kind}: {case}: {why}");
            return None;
        }
        return Some(live::over(forge_kind));
    }
    let text = recording(kind, case);
    Some(match how {
        How::Recorded => {
            let recorded = Arc::new(Recorded::parse(&text).unwrap());
            let forge = Forge::default_of(forge_kind);
            let check = recorded.clone();
            Over {
                backend: forge.backend_over(recorded),
                caller: caller(),
                host: forge.host.clone(),
                scene: Scene::recorded(forge_kind),
                check: Box::new(move || check.spent()),
                sweep: Box::new(|| Ok(Vec::new())),
            }
        }
        How::Native => native::over(kind, default_host(kind), &text),
        How::SelfManaged => {
            // A case a self-managed instance is asked differently, because a capability charter
            // has not probed there takes its fallback, has its own recording beside the case's.
            let own = recordings()
                .join(kind)
                .join(format!("{case}.self_managed.json"));
            let text = own_recording(&own).unwrap_or(text);
            native::over(kind, SELF_MANAGED, &text)
        }
        How::Live => unreachable!("answered above"),
    })
}

/// The host a self-managed instance is on, in the native cases that run on one.
const SELF_MANAGED: &str = "forge.example.com";

/// The forge's own instance.
fn default_host(kind: &str) -> &'static str {
    match kind {
        "github" => "github.com",
        _ => "gitlab.com",
    }
}

fn caller() -> Caller {
    Caller::command()
}

mod cases {
    use super::*;

    /// A listing in path order: neither forge's default order is part of the contract.
    fn by_path(
        mut repos: Vec<purlis_core::forge::RepoRecord>,
    ) -> Vec<purlis_core::forge::RepoRecord> {
        repos.sort_by(|a, b| a.path_with_namespace.cmp(&b.path_with_namespace));
        repos
    }

    pub fn owned(kind: &str, how: How) {
        let Some(over) = over(kind, "owned", how) else {
            return;
        };
        let scene = &over.scene;
        let repos = over
            .backend
            .owned(&over.caller, &Owner::new(&scene.owner))
            .unwrap();
        assert_eq!(
            by_path(repos),
            [scene.api(), scene.web()],
            "every field typed; a null description is empty"
        );
        spent(&over);
    }

    pub fn reachable(kind: &str, how: How) {
        let Some(over) = over(kind, "reachable", how) else {
            return;
        };
        let scene = &over.scene;
        let repos = over
            .backend
            .reachable(&over.caller, &Owner::new(&scene.owner))
            .unwrap();
        assert_eq!(
            repos,
            [scene.api()],
            "only the declared owner's repos the account reaches"
        );
        spent(&over);
    }

    pub fn about(kind: &str, how: How) {
        let Some(over) = over(kind, "about", how) else {
            return;
        };
        // Asked of the repo itself, as the inventory reads it: no forge id, which `about` never
        // needs, since both forges address the repo by its path.
        let repo = purlis_core::forge::RepoRecord {
            id: None,
            ..over.scene.api()
        };
        assert_eq!(
            over.backend.about(&over.caller, &repo),
            Ok(About {
                visibility: Visibility::Private,
                issues: Issues::Open,
                // The account is no admin, so neither forge names the setting.
                push_protection: PushProtection::Unknown,
            })
        );
        spent(&over);
    }

    pub fn top_level(kind: &str, how: How) {
        let Some(over) = over(kind, "top_level", how) else {
            return;
        };
        let scene = &over.scene;
        let repo = scene.repo(&scene.tree_id, "api", &[]);
        assert_eq!(
            over.backend.top_level(&over.caller, &repo, None).unwrap(),
            ["Cargo.toml", "src"]
        );
        spent(&over);
    }

    pub fn open_or_update(kind: &str, how: How) {
        let Some(over) = over(kind, "open_or_update", how) else {
            return;
        };
        let scene = &over.scene;
        let opened = over
            .backend
            .open_or_update(
                &over.caller,
                &scene.path("api"),
                &scene.opened_from,
                "main",
                "Save",
                "<!-- charter-save -->",
            )
            .unwrap();
        // On a live forge the first night opens the request and every later night updates it;
        // its number is the forge's to give.
        let number = scene.opened.unwrap_or(opened.pr.number);
        assert_eq!(
            opened,
            Opened {
                pr: Pr {
                    number,
                    url: scene.request_url(number)
                },
                ours: true
            }
        );
        spent(&over);
    }

    pub fn state(kind: &str, how: How) {
        let Some(over) = over(kind, "state", how) else {
            return;
        };
        let scene = &over.scene;
        let pr = Pr {
            number: scene.merged,
            url: String::new(),
        };
        assert_eq!(
            over.backend.state(&over.caller, &scene.path("api"), &pr),
            Ok(State::Merged {
                commit: Some(scene.merge.clone())
            })
        );
        spent(&over);
    }

    /// The body a request's description holds, as `charter change push` splices it.
    pub const BODY: &str = "Why this change.\n\n<!-- END charter change -->";

    /// The scene's open request.
    fn open(scene: &Scene) -> Pr {
        Pr {
            number: scene.open,
            url: String::new(),
        }
    }

    pub fn body(kind: &str, how: How) {
        let Some(over) = over(kind, "body", how) else {
            return;
        };
        let scene = &over.scene;
        assert_eq!(
            over.backend
                .body(&over.caller, &scene.path("api"), &open(scene)),
            Ok(BODY.to_string())
        );
        spent(&over);
    }

    pub fn set_body(kind: &str, how: How) {
        let Some(over) = over(kind, "set_body", how) else {
            return;
        };
        let scene = &over.scene;
        assert_eq!(
            over.backend
                .set_body(&over.caller, &scene.path("api"), &open(scene), BODY),
            Ok(())
        );
        spent(&over);
    }

    pub fn by_head(kind: &str, how: How) {
        let Some(over) = over(kind, "by_head", how) else {
            return;
        };
        let scene = &over.scene;
        let found = over
            .backend
            .by_head(&over.caller, &scene.path("api"), SAVE)
            .unwrap()
            .expect("a request");
        assert_eq!(
            (found.number, found.state, found.head.as_str()),
            (scene.open, State::Open, scene.head.as_str())
        );
        let _: Request = found;
        spent(&over);
    }

    pub fn request_auto_merge(kind: &str, how: How) {
        let Some(over) = over(kind, "request_auto_merge", how) else {
            return;
        };
        let scene = &over.scene;
        assert_eq!(
            over.backend.request_auto_merge(
                &over.caller,
                &scene.path("api"),
                &open(scene),
                &scene.head
            ),
            Ok(AutoMerge::Queued)
        );
        spent(&over);
    }

    /// How every landing case asks to merge: a merge commit, carrying the trailer.
    fn merge_as(scene: &Scene) -> MergeAs {
        let sigil = scene.kind.change_sigil();
        MergeAs {
            squash: false,
            title: format!("api-2: api ({sigil}{})", scene.open),
            message: "bump the api\n\nCharter-Change: api-2".to_string(),
        }
    }

    pub fn lands_through_queue(kind: &str, how: How) {
        let Some(over) = over(kind, "lands_through_queue", how) else {
            return;
        };
        let scene = &over.scene;
        assert_eq!(
            over.backend
                .lands_through_queue(&over.caller, &scene.path("api"), &open(scene)),
            Ok(scene.queue)
        );
        spent(&over);
    }

    pub fn merge_at(kind: &str, how: How) {
        let Some(over) = over(kind, "merge_at", how) else {
            return;
        };
        let scene = &over.scene;
        assert_eq!(
            over.backend.merge_at(
                &over.caller,
                &scene.path("api"),
                &open(scene),
                &scene.head,
                &merge_as(scene)
            ),
            Ok(MergedAt::Now)
        );
        spent(&over);
    }

    pub fn enqueue_at(kind: &str, how: How) {
        let Some(over) = over(kind, "enqueue_at", how) else {
            return;
        };
        let scene = &over.scene;
        assert_eq!(
            over.backend.enqueue_at(
                &over.caller,
                &scene.path("api"),
                &open(scene),
                &scene.head,
                &merge_as(scene)
            ),
            Ok(())
        );
        spent(&over);
    }

    pub fn checks_at(kind: &str, how: How) {
        let Some(over) = over(kind, "checks_at", how) else {
            return;
        };
        let scene = &over.scene;
        assert_eq!(
            over.backend
                .checks_at(&over.caller, &scene.path("api"), &scene.head, scene.open),
            Checks {
                total: Some(1),
                ci: Ci::Passed,
                why: None
            }
        );
        spent(&over);
    }

    pub fn open_on_branch(kind: &str, how: How) {
        let Some(over) = over(kind, "open_on_branch", how) else {
            return;
        };
        let scene = &over.scene;
        assert_eq!(
            over.backend
                .open_on_branch(&over.caller, &scene.path("api"), SAVE),
            Ok(Some(Value::from(scene.open)))
        );
        spent(&over);
    }

    pub fn ci_word(kind: &str, how: How) {
        let Some(over) = over(kind, "ci_word", how) else {
            return;
        };
        let scene = &over.scene;
        assert_eq!(
            over.backend.ci_word(&over.caller, &scene.path("api"), SAVE),
            Ok(Some("success".to_string()))
        );
        spent(&over);
    }

    pub fn support(kind: &str, how: How) {
        let Some(over) = over(kind, "support", how) else {
            return;
        };
        let backend = &over.backend;
        for what in Capability::ALL {
            let at = Reach::Repo(over.scene.path("api"));
            assert_ne!(
                backend.support(&over.caller, &at, what),
                Support::Available,
                "{what:?}: what charter has not asked a repo about is never yes"
            );
        }
        // Epics, said of the instance: GitHub has none anywhere; gitlab.com has them (on a
        // Premium namespace, which a narrower reach would have to probe); a self-managed GitLab
        // has what its edition and licence have, which charter has not asked.
        let epics = backend.support(&over.caller, &Reach::Instance, Capability::Epics);
        // Sub-issues, said of the instance: github.com has them; a GitHub Enterprise Server has
        // what its version has, which charter has not asked.
        let sub_issues = backend.support(&over.caller, &Reach::Instance, Capability::SubIssues);
        match (kind, over.host.as_str()) {
            ("github", "github.com") => {
                assert!(matches!(epics, Support::Unavailable(_)), "{epics:?}");
                assert_eq!(sub_issues, Support::Available);
            }
            ("github", _) => {
                assert!(matches!(epics, Support::Unavailable(_)), "{epics:?}");
                assert!(matches!(sub_issues, Support::Unknown(_)), "{sub_issues:?}");
            }
            ("gitlab", "gitlab.com") => assert_eq!(epics, Support::Available),
            ("gitlab", _) => assert!(matches!(epics, Support::Unknown(_)), "{epics:?}"),
            _ => unreachable!("two forges"),
        }
        spent(&over);
    }

    pub fn create(kind: &str, how: How) {
        let Some(over) = over(kind, "create", how) else {
            return;
        };
        let scene = &over.scene;
        // Swept before, for what an earlier run left, and after, whatever `create` answered.
        (over.sweep)().unwrap_or_else(|e| panic!("sweeping before the case: {e}"));
        let made = over.backend.create(
            &over.caller,
            &scene.path("api"),
            &NewWorkItem {
                title: live::ISSUE_TITLE.into(),
                body: "What the todo said.\n\nSecond paragraph.".into(),
                workspace_label: Some("alpha".into()),
            },
        );
        let swept = (over.sweep)();
        let made = made.unwrap();
        let swept = swept.unwrap_or_else(|e| panic!("sweeping after the case: {e}"));
        // On a live forge the issue's number and id are the forge's to give: the number is read
        // off the page the answer names, must agree with its key, and must be one the sweep
        // found open under the case's title.
        let (number, forge_ref) = match &scene.created {
            Some((number, forge_ref)) => (*number, Some(forge_ref.clone())),
            None => {
                let number = made
                    .url
                    .rsplit('/')
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_else(|| panic!("no issue number ends {}", made.url));
                assert!(swept.contains(&number), "{number} not among {swept:?}");
                (number, None)
            }
        };
        assert_eq!(made.key.as_str(), scene.issue_key(number));
        assert_eq!(made.url, scene.issue_url(number));
        // The answer, in the neutral model (FW-5): what a board draws the new issue's card from.
        assert_eq!(made.title, live::ISSUE_TITLE);
        assert_eq!(made.kind, work::Kind::Issue);
        assert_eq!(made.state, work::State::Open);
        let label = match kind {
            "github" => "ws:alpha",
            _ => "charter::ws::alpha",
        };
        assert_eq!(made.labels, [label]);
        assert!(!made.is_sub_issue());
        assert_eq!(
            made.milestone, None,
            "the case opens its issue in no milestone"
        );
        match forge_ref {
            Some(forge_ref) => assert_eq!(made.forge_ref, Some(ForgeRef(forge_ref))),
            None => assert!(
                made.forge_ref.as_ref().is_some_and(|r| !r.0.is_empty()),
                "{:?}",
                made.forge_ref
            ),
        }
        spent(&over);
    }

    pub fn read(kind: &str, how: How) {
        let Some(over) = over(kind, "read", how) else {
            return;
        };
        let scene = &over.scene;
        let item = over
            .backend
            .read(&over.caller, &scene.path("api"), 12)
            .unwrap();
        // What both forges answer: the issue, its fields, and who it is assigned to.
        assert_eq!(item.key.as_str(), scene.issue_key(12));
        assert_eq!(item.url, scene.issue_url(12));
        assert_eq!(item.title, live::ISSUE_TITLE);
        assert_eq!(item.kind, work::Kind::Issue);
        assert_eq!(item.state, work::State::Closed);
        assert_eq!(
            item.milestone.as_ref().map(|m| m.title.as_str()),
            Some("v1")
        );
        assert_eq!(item.assignees, ["octocat"]);
        let key = |number: u64| work::TrackerKey::parse(&scene.issue_key(number)).unwrap();
        // How it stands to other items, the same on both forges: GitHub's parent, sub-issue,
        // dependencies and closing pull request; GitLab's hierarchy, blocking links and closing
        // merge request.
        assert_eq!(
            item.relations,
            [
                work::Relation::Parent(key(3)),
                work::Relation::Child(key(14)),
                work::Relation::BlockedBy(key(11)),
                work::Relation::Blocks(key(15)),
                work::Relation::ClosedBy(Pr {
                    number: 20,
                    url: scene.request_url(20),
                }),
            ]
        );
        let day = |d| chrono::NaiveDate::from_ymd_opt(2026, 10, d);
        let sprint = item.iteration.as_ref().expect("the item's iteration");
        assert_eq!(sprint.title.as_deref(), Some("Sprint 3"));
        assert_eq!((sprint.start, sprint.end), (day(5), day(18)));
        assert_eq!(item.placements.len(), 1);
        assert_eq!(item.placements[0].board_title, "Roadmap");
        // A status of the item's own, and the close reason its category gives, are asked only
        // where GitLab's work item status is known to be there: not of a self-managed GitLab,
        // which charter has not probed. There the item has its boards' status, and a closed
        // issue sits in no label list.
        let own_status = !(kind == "gitlab" && over.host != "gitlab.com");
        if own_status {
            assert_eq!(item.status.as_deref(), Some("Done"));
            assert_eq!(item.closed_as, Some(work::ClosedAs::Completed));
        } else {
            assert_eq!((item.status.as_deref(), item.closed_as), (None, None));
        }
        match kind {
            "github" => {
                assert_eq!(item.labels, ["ws:alpha"]);
                assert_eq!(item.issue_type.as_deref(), Some("Bug"));
                // An issue has no status of its own on GitHub: it is its board's.
                assert_eq!(item.placements[0].status.as_deref(), Some("Done"));
                assert_eq!(
                    item.placements[0].iteration.as_ref(),
                    Some(sprint),
                    "the item's iteration is its board's"
                );
            }
            _ => {
                assert_eq!(item.labels, ["charter::ws::alpha"]);
                // GitLab's work item type, and its own iteration and status (Premium). A closed
                // issue sits in no label list of a board, and its close reason is its status's
                // category.
                assert_eq!(item.issue_type.as_deref(), Some("Issue"));
                assert_eq!(item.placements[0].status, None);
                assert_eq!(item.placements[0].iteration, None);
            }
        }
        spent(&over);
    }
}

/// One module per forge, holding every case of [`CASES`]. `ignore = "<why>"` ignores each.
macro_rules! contract {
    ($module:ident, $forge:ident, $how:ident) => {
        contract!(@cases $module, $forge, $how,);
    };
    ($module:ident, $forge:ident, $how:ident, ignore = $why:literal) => {
        contract!(@cases $module, $forge, $how, ignore = $why);
    };
    (@cases $module:ident, $forge:ident, $how:ident, $($attr:meta)?) => {
        mod $module {
            #[test]
            $(#[$attr])?
            fn owned_lists_every_repo_of_the_owner_in_the_neutral_shape() {
                purlis_core::unsteered!();
                super::cases::owned(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn reachable_keeps_only_the_declared_owners_repos() {
                purlis_core::unsteered!();
                super::cases::reachable(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn top_level_lists_the_default_branchs_top_level_names() {
                purlis_core::unsteered!();
                super::cases::top_level(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn open_or_update_opens_one_request_when_none_is_open() {
                purlis_core::unsteered!();
                super::cases::open_or_update(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn state_reads_a_merged_request_with_the_commit_it_landed_as() {
                purlis_core::unsteered!();
                super::cases::state(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn body_reads_the_requests_description_whole() {
                purlis_core::unsteered!();
                super::cases::body(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn set_body_replaces_the_requests_description_and_nothing_else() {
                purlis_core::unsteered!();
                super::cases::set_body(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn by_head_finds_the_request_from_the_branch_with_its_head_commit() {
                purlis_core::unsteered!();
                super::cases::by_head(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn request_auto_merge_queues_the_merge_at_the_pushed_commit() {
                purlis_core::unsteered!();
                super::cases::request_auto_merge(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn lands_through_queue_reads_whether_the_target_branch_has_a_queue() {
                purlis_core::unsteered!();
                super::cases::lands_through_queue(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn merge_at_merges_now_only_at_the_head_charter_read() {
                purlis_core::unsteered!();
                super::cases::merge_at(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn enqueue_at_puts_the_request_in_the_queue_only_at_the_head_charter_read() {
                purlis_core::unsteered!();
                super::cases::enqueue_at(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn checks_at_counts_one_passing_check_at_the_head() {
                purlis_core::unsteered!();
                super::cases::checks_at(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn open_on_branch_names_the_open_requests_number() {
                purlis_core::unsteered!();
                super::cases::open_on_branch(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn ci_word_maps_the_forges_success_to_success() {
                purlis_core::unsteered!();
                super::cases::ci_word(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn support_never_reads_silence_as_yes() {
                purlis_core::unsteered!();
                super::cases::support(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn about_says_whether_the_repo_is_public_and_takes_issues() {
                purlis_core::unsteered!();
                super::cases::about(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn create_opens_an_issue_and_names_it_by_its_tracker_key_with_its_forge_ref_beside() {
                purlis_core::unsteered!();
                super::cases::create(stringify!($forge), super::How::$how);
            }
            #[test]
            $(#[$attr])?
            fn read_maps_an_issue_onto_the_work_model_with_its_relations_and_boards() {
                purlis_core::unsteered!();
                super::cases::read(stringify!($forge), super::How::$how);
            }
        }
    };
}

contract!(github, github, Recorded);
contract!(gitlab, gitlab, Recorded);
// The native transport, against a server answering the same recordings over HTTP: on each
// forge's own instance, and on a self-managed instance of each: a GitHub Enterprise Server and a
// self-managed GitLab (ADR 0070 §7: it gates every change).
contract!(github_native, github, Native);
contract!(gitlab_native, gitlab, Native);
contract!(github_self_managed, github, SelfManaged);
contract!(gitlab_self_managed, gitlab, SelfManaged);
// The live run: the same cases against a real forge (FG-4). Only the nightly holds the tokens.
contract!(
    github_live,
    github,
    Live,
    ignore = "live: a real github.com repo; run by .github/workflows/forge-live.yml"
);
contract!(
    gitlab_live,
    gitlab,
    Live,
    ignore = "live: a real gitlab.com repo; run by .github/workflows/forge-live.yml"
);

/// The method names of one `pub trait <name> { … }` block in `backend.rs`.
fn methods_of(source: &str, name: &str) -> Vec<String> {
    let start = source
        .find(&format!("pub trait {name} {{"))
        .unwrap_or_else(|| panic!("backend.rs declares no trait {name}"));
    let body = &source[start..];
    let end = body.find("\n}\n").expect("the trait's closing brace");
    body[..end]
        .lines()
        .filter_map(|line| line.trim().strip_prefix("fn "))
        .map(|rest| rest.split('(').next().unwrap().trim().to_string())
        .collect()
}

#[test]
fn every_method_of_the_seam_has_a_case_on_both_forges() {
    purlis_core::unsteered!();
    let source =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/forge/backend.rs"))
            .unwrap();
    let mut methods: Vec<String> = ["Repos", "Requests", "Capabilities", "WorkItems"]
        .iter()
        .flat_map(|t| methods_of(&source, t))
        .collect();
    methods.sort();
    let mut cases: Vec<String> = CASES.iter().map(|c| c.to_string()).collect();
    cases.sort();
    assert_eq!(
        methods, cases,
        "a seam method with no contract case, or the reverse"
    );
    for kind in ["github", "gitlab"] {
        for case in CASES {
            let file = recordings().join(kind).join(format!("{case}.json"));
            assert!(file.is_file(), "no {kind} recording for {case}");
        }
    }
    // The macro gives both forges the same tests; this file holds exactly one instantiation per
    // forge, so neither can drift from the other.
    let me = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/forge_contract.rs"),
    )
    .unwrap();
    for kind in ["github", "gitlab"] {
        assert_eq!(
            me.matches(&format!("contract!({kind}, {kind}, Recorded);"))
                .count(),
            1,
            "{kind} is instantiated once"
        );
    }
    for (module, kind, how) in [
        ("github_native", "github", "Native"),
        ("gitlab_native", "gitlab", "Native"),
        ("github_self_managed", "github", "SelfManaged"),
        ("gitlab_self_managed", "gitlab", "SelfManaged"),
    ] {
        assert_eq!(
            me.matches(&format!("contract!({module}, {kind}, {how});"))
                .count(),
            1,
            "the native transport runs every case: {module}"
        );
    }
    // The live run (FG-4) is the same macro over a real forge: one instantiation per forge, ignored
    // so that only the nightly, which holds the tokens, asks for it.
    let flat: String = me.split_whitespace().collect();
    for kind in ["github", "gitlab"] {
        assert_eq!(
            flat.matches(&format!("contract!({kind}_live,{kind},Live,ignore="))
                .count(),
            1,
            "the live run runs every case on {kind}"
        );
    }
}

/// Every recording names a case of [`CASES`], by its stem with or without `.self_managed`. A
/// self-managed override whose case was renamed would otherwise go unread, and the self-managed
/// run would quietly fall back to the case's main recording.
#[test]
fn every_recording_on_both_forges_names_a_case() {
    purlis_core::unsteered!();
    for kind in ["github", "gitlab"] {
        let dir = recordings().join(kind);
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let stem = name
                .strip_suffix(".json")
                .unwrap_or_else(|| panic!("{kind}: {name} is no recording"));
            let case = stem.strip_suffix(".self_managed").unwrap_or(stem);
            assert!(
                CASES.contains(&case),
                "{kind}: {name} names no case of the contract"
            );
        }
    }
}

/// A self-managed override that is not there is no override; one that is there but cannot be
/// read fails the case rather than passing it on the main recording.
#[test]
fn a_self_managed_override_that_cannot_be_read_fails_rather_than_falling_back() {
    purlis_core::unsteered!();
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        own_recording(&dir.path().join("absent.self_managed.json")),
        None
    );
    let file = dir.path().join("there.self_managed.json");
    std::fs::write(&file, "{}").unwrap();
    assert_eq!(own_recording(&file).as_deref(), Some("{}"));
    // A directory in the file's place is there, and reading it is an error, not a miss.
    let unreadable = dir.path().join("dir.self_managed.json");
    std::fs::create_dir(&unreadable).unwrap();
    let said = std::panic::catch_unwind(|| own_recording(&unreadable));
    assert!(
        said.is_err(),
        "an unreadable override passed as no override"
    );
}

/// A case the live run does not run says why, and names the ticket that will run it (ADR 0070
/// §7: a test is never skipped silently).
#[test]
fn every_case_the_live_run_leaves_out_says_why_and_names_its_ticket() {
    purlis_core::unsteered!();
    let ticket = regex::Regex::new(r"#\d+\b").unwrap();
    for (case, why) in NOT_LIVE {
        assert!(CASES.contains(&case), "{case} is no case of the contract");
        assert!(
            ticket.is_match(why),
            "{case}: the reason names no ticket: {why}"
        );
    }
    assert!(
        NOT_LIVE.len() < CASES.len() / 2,
        "the live run leaves out most of the contract"
    );
}

/// GitLab statuses its own documentation lists that the Python port did not (GitLab 19.4
/// `doc/api/pipelines.md`, the `status` filter: thirteen values). These are GitLab's alone, so
/// they are not parity cases.
mod gitlab_statuses {
    use super::*;

    /// A backend over an inline recording.
    fn inline(exchanges: Value) -> (Box<dyn ForgeBackend>, Arc<Recorded>) {
        let text =
            json!({"source": "GitLab 19.4 REST API docs, pipelines.md", "exchanges": exchanges});
        let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
        let forge = Forge::default_of(purlis_core::forge::Kind::GitLab);
        (forge.backend_over(recorded.clone()), recorded)
    }

    fn get(path: &str, out: Value) -> Value {
        json!({"call": {"endpoint": {"rest": {"method": null, "path": path}}, "fields": []},
               "reply": {"code": 0, "out": out.to_string()}})
    }

    fn ci_word_of(status: &str) -> Option<String> {
        let (backend, recorded) = inline(json!([get(
            "projects/acme%2Fapi/pipelines?ref=main&per_page=1",
            json!([{"id": 1, "status": status}])
        )]));
        let word = backend.ci_word(&caller(), "acme/api", "main").unwrap();
        spent(&recorded);
        word
    }

    fn checks_of(status: &str) -> Ci {
        let (backend, recorded) = inline(json!([get(
            "projects/acme%2Fapi/merge_requests/12/pipelines?per_page=100",
            json!([{"id": 1, "sha": SHA, "status": status}])
        )]));
        let checks = backend.checks_at(&caller(), "acme/api", SHA, 12);
        spent(&recorded);
        checks.ci
    }

    #[test]
    fn a_pipeline_waiting_for_a_callback_is_pending_and_still_running() {
        purlis_core::unsteered!();
        assert_eq!(ci_word_of("waiting_for_callback"), Some("pending".into()));
        assert_eq!(checks_of("waiting_for_callback"), Ci::Running);
    }

    #[test]
    fn a_pipeline_being_canceled_reads_as_canceled_and_did_not_pass() {
        purlis_core::unsteered!();
        assert_eq!(ci_word_of("canceling"), Some("canceled".into()));
        assert_eq!(checks_of("canceling"), Ci::Failed);
    }

    #[test]
    fn auto_merge_waits_on_a_pipeline_waiting_for_a_callback() {
        purlis_core::unsteered!();
        let (backend, recorded) = inline(json!([
            get("projects/acme%2Fapi", json!({"squash_option": "never"})),
            get(
                "projects/acme%2Fapi/merge_requests/12",
                json!({"iid": 12, "head_pipeline": {"status": "waiting_for_callback"}})
            ),
            {"call": {"endpoint": {"rest": {"method": "PUT",
                                            "path": "projects/acme%2Fapi/merge_requests/12/merge"}},
                      "fields": [{"typed": ["merge_when_pipeline_succeeds", "true"]},
                                 {"typed": ["squash", "false"]},
                                 {"text": ["sha", SHA]}]},
             "reply": {"code": 0, "out": "{\"iid\": 12}"}}
        ]));
        let pr = Pr {
            number: 12,
            url: String::new(),
        };
        assert_eq!(
            backend.request_auto_merge(&caller(), "acme/api", &pr, SHA),
            Ok(AutoMerge::Queued)
        );
        spent(&recorded);
    }
}

/// GitLab matches `source_branch` by name in every project, forks included
/// (`doc/api/merge_requests.md`, List project merge requests). A merge request from a fork's
/// branch of the same name is not the branch's.
mod gitlab_forks {
    use super::*;

    fn inline(exchanges: Value) -> (Box<dyn ForgeBackend>, Arc<Recorded>) {
        let text = json!({"source": "GitLab 19.4 REST API docs, merge_requests.md",
                          "exchanges": exchanges});
        let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
        let forge = Forge::default_of(purlis_core::forge::Kind::GitLab);
        (forge.backend_over(recorded.clone()), recorded)
    }

    fn listing(out: Value) -> Value {
        json!([{"call": {"endpoint": {"rest": {"method": null,
                    "path": "projects/acme%2Fapi/merge_requests?state=opened&source_branch=main&per_page=100"}},
                 "fields": []},
                "reply": {"code": 0, "out": out.to_string()}}])
    }

    #[test]
    fn the_status_line_never_shows_a_forks_merge_request() {
        purlis_core::unsteered!();
        let (backend, recorded) = inline(listing(json!([
            {"iid": 99, "source_project_id": 9, "target_project_id": 3},
            {"iid": 12, "source_project_id": 3, "target_project_id": 3}
        ])));
        assert_eq!(
            backend.open_on_branch(&caller(), "acme/api", "main"),
            Ok(Some(Value::from(12)))
        );
        spent(&recorded);
    }

    #[test]
    fn only_forks_merge_requests_on_the_branch_is_no_open_request() {
        purlis_core::unsteered!();
        let (backend, recorded) = inline(listing(json!([
            {"iid": 99, "source_project_id": 9, "target_project_id": 3}
        ])));
        assert_eq!(
            backend.open_on_branch(&caller(), "acme/api", "main"),
            Ok(None)
        );
        spent(&recorded);
    }
}

/// Whether an account can open an issue, in each forge's own fields (`Repos::about`): GitHub's
/// `has_issues` and `permissions.pull`, GitLab's `issues_access_level` and membership.
mod who_can_open_an_issue {
    use super::*;

    fn about_of(kind: purlis_core::forge::Kind, path: &str, out: Value) -> About {
        let text = json!({"source": "GitHub REST 2022-11-28 repos.md; GitLab 19.4 projects.md",
            "exchanges": [{"call": {"endpoint": {"rest": {"method": null, "path": path}},
                                    "fields": []},
                           "reply": {"code": 0, "out": out.to_string()}}]});
        let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
        let backend = Forge::default_of(kind).backend_over(recorded.clone());
        let repo = purlis_core::forge::RepoRecord {
            id: None,
            name: "api".into(),
            path_with_namespace: "acme/api".into(),
            default_branch: None,
            description: String::new(),
            web_url: String::new(),
            ssh_url: String::new(),
            topics: Vec::new(),
            forge: kind,
        };
        let about = backend.about(&caller(), &repo).unwrap();
        spent(&recorded);
        about
    }

    #[test]
    fn github_says_off_when_issues_are_off_and_no_right_when_it_may_not_read() {
        purlis_core::unsteered!();
        let github = |out| about_of(purlis_core::forge::Kind::GitHub, "repos/acme/api", out);
        assert_eq!(
            github(json!({"private": false, "has_issues": false})),
            About {
                visibility: Visibility::Public,
                issues: Issues::Off,
                push_protection: PushProtection::Unknown,
            }
        );
        assert_eq!(
            github(json!({"visibility": "internal", "permissions": {"pull": false}})).issues,
            Issues::NoRight
        );
    }

    #[test]
    fn gitlab_says_no_right_for_members_only_issues_to_a_stranger() {
        purlis_core::unsteered!();
        let gitlab = |out| about_of(purlis_core::forge::Kind::GitLab, "projects/acme%2Fapi", out);
        let stranger = json!({"visibility": "public", "issues_access_level": "private",
                              "permissions": {"project_access": null, "group_access": null}});
        assert_eq!(gitlab(stranger).issues, Issues::NoRight);
        let member = json!({"visibility": "internal", "issues_access_level": "private",
                            "permissions": {"project_access": null,
                                            "group_access": {"access_level": 10}}});
        assert_eq!(
            gitlab(member),
            About {
                visibility: Visibility::Internal,
                issues: Issues::Open,
                push_protection: PushProtection::Unknown,
            }
        );
        let off = json!({"visibility": "private", "issues_access_level": "disabled"});
        assert_eq!(gitlab(off).issues, Issues::Off);
    }

    /// Whether the forge refuses a push that carries a secret (SQ-8), in each forge's own field:
    /// GitHub's `security_and_analysis.secret_scanning_push_protection.status`, GitLab's
    /// `secret_push_protection_enabled` (`pre_receive_secret_detection_enabled` before 18.0).
    /// Both forges show it only to an account that may change it; silence is `Unknown`.
    #[test]
    fn push_protection_is_read_from_each_forges_own_field_and_silence_is_unknown() {
        purlis_core::unsteered!();
        let github =
            |out| about_of(purlis_core::forge::Kind::GitHub, "repos/acme/api", out).push_protection;
        let status = |s: &str| {
            json!({"visibility": "public", "security_and_analysis":
                   {"secret_scanning_push_protection": {"status": s}}})
        };
        assert_eq!(github(status("enabled")), PushProtection::On);
        assert_eq!(github(status("disabled")), PushProtection::Off);
        assert_eq!(github(status("something new")), PushProtection::Unknown);
        assert_eq!(
            github(json!({"visibility": "public"})),
            PushProtection::Unknown
        );

        let gitlab = |out| {
            about_of(purlis_core::forge::Kind::GitLab, "projects/acme%2Fapi", out).push_protection
        };
        let public = |key: &str, on: bool| json!({"visibility": "public", key: on});
        assert_eq!(
            gitlab(public("secret_push_protection_enabled", true)),
            PushProtection::On
        );
        assert_eq!(
            gitlab(public("secret_push_protection_enabled", false)),
            PushProtection::Off
        );
        assert_eq!(
            gitlab(public("pre_receive_secret_detection_enabled", true)),
            PushProtection::On
        );
        assert_eq!(
            gitlab(json!({"visibility": "public"})),
            PushProtection::Unknown
        );
    }
}

/// The network log, for the recorded REST calls of the parity table on both forges (FG-3): each
/// is listed by its template, and no name of the scene (the owner, the repo, a branch, a
/// request's number, a commit) survives into it. A GraphQL call is listed as `graphql`, which
/// holds no name, so it is not read here.
mod network_log {
    use super::*;

    /// Every name the recordings' paths carry, however each forge spells it.
    const SCENE: [&str; 10] = [
        "acme", "api", "web", "main", "charter", "save", "12", "7", SHA, MERGE,
    ];

    /// Each case's recording, then its self-managed override where there is one, so a path only
    /// an override asks is checked too.
    fn paths(kind: &str) -> Vec<String> {
        CASES
            .iter()
            .flat_map(|case| {
                let own = recordings()
                    .join(kind)
                    .join(format!("{case}.self_managed.json"));
                std::iter::once(recording(kind, case)).chain(own_recording(&own))
            })
            .flat_map(|text| {
                let file: Value = serde_json::from_str(&text).unwrap();
                file["exchanges"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|e| {
                        e["call"]["endpoint"]["rest"]["path"]
                            .as_str()
                            .map(String::from)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn every_recorded_call_on_both_forges_is_listed_with_no_name_of_the_scene() {
        purlis_core::unsteered!();
        for kind in ["github", "gitlab"] {
            let paths = paths(kind);
            // Exactly the REST calls the recordings and their overrides hold, so a recording that
            // lost one, or a reader that skipped some, fails here rather than checking less.
            let expected = match kind {
                "github" => 18,
                _ => 24,
            };
            assert_eq!(paths.len(), expected, "{kind}: the recorded REST calls");
            for path in paths {
                let listed = purlis_core::netlog::template(&path);
                assert!(
                    !listed.contains('?'),
                    "{kind}: {path} keeps its query: {listed}"
                );
                for segment in listed.split('/') {
                    assert!(
                        !SCENE.iter().any(|name| segment.contains(name)),
                        "{kind}: {path} is listed as {listed}, which names {segment}"
                    );
                }
            }
        }
    }
}

/// GitHub's `owned` for a personal account: the org endpoint answers `404`, and the same listing
/// is asked of the user endpoint (GitHub REST `2022-11-28`, "List repositories for a user").
/// GitLab's counterpart is [`gitlab_user_namespace`].
mod github_personal_account {
    use super::*;

    fn page(path: &str, reply: Value) -> Value {
        json!({"call": {"endpoint": {"rest": {"method": null, "path": path}}, "fields": []},
               "reply": reply})
    }

    #[test]
    fn owned_falls_back_to_the_user_endpoint_on_a_404_and_answers_the_same_shape() {
        purlis_core::unsteered!();
        let repos = json!([{"id": 1, "name": "api", "full_name": "acme/api",
            "default_branch": "main", "description": null,
            "html_url": "https://github.com/acme/api",
            "ssh_url": "git@github.com:acme/api.git", "topics": []}]);
        let text = json!({"source": "GitHub REST 2022-11-28, repos: List organization \
                                 repositories (404 for a user), List repositories for a user",
        "exchanges": [
            page("orgs/acme/repos?per_page=100&page=1",
                 json!({"code": 1, "out": "{\"message\":\"Not Found\",\"status\":\"404\"}",
                        "err": "gh: Not Found (HTTP 404)", "status": 404})),
            page("users/acme/repos?per_page=100&page=1",
                 json!({"code": 0, "out": repos.to_string()})),
        ]});
        let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
        let backend = Forge::default_of(Kind::GitHub).backend_over(recorded.clone());
        let owned = backend.owned(&caller(), &Owner::new("acme")).unwrap();
        assert_eq!(owned.len(), 1);
        assert_eq!(owned[0].path_with_namespace, "acme/api");
        assert_eq!(owned[0].forge, Kind::GitHub);
        spent(&recorded);
    }
}

/// GitLab's `owned` for a user namespace (#803): the group endpoint answers `404 Group Not
/// Found`, and the same listing is asked of the user endpoint (GitLab 19.4 `doc/api/projects.md`,
/// "List all personal projects for a user"), as GitHub falls back from an organisation to a
/// user. A group is still asked first, and only a not-found answer falls back.
mod gitlab_user_namespace {
    use super::*;
    use purlis_core::forge::Failure;

    const GROUP: &str = "groups/solo/projects?per_page=100&page=1&include_subgroups=true\
                         &archived=false&with_shared=false";
    const USER: &str = "users/solo/projects?per_page=100&page=1&archived=false";

    fn page(path: &str, reply: Value) -> Value {
        json!({"call": {"endpoint": {"rest": {"method": null, "path": path}}, "fields": []},
               "reply": reply})
    }

    fn over(exchanges: Value) -> (Box<dyn ForgeBackend>, Arc<Recorded>) {
        let text = json!({"source": "GitLab 19.4 REST API docs, groups.md: List projects (404 \
                                     for a user), projects.md: List all personal projects \
                                     for a user",
                          "exchanges": exchanges});
        let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
        let backend = Forge::default_of(Kind::GitLab).backend_over(recorded.clone());
        (backend, recorded)
    }

    fn personal() -> Value {
        let repos = json!([{"id": 3, "name": "Dots", "path": "dots",
            "path_with_namespace": "solo/dots", "default_branch": "main", "description": null,
            "web_url": "https://gitlab.com/solo/dots",
            "ssh_url_to_repo": "git@gitlab.com:solo/dots.git", "topics": []}]);
        json!({"code": 0, "out": repos.to_string()})
    }

    /// GitLab's answer for a group that does not exist, as `glab api` says it and as the
    /// native transport sees it.
    fn no_such_group() -> [Value; 2] {
        let body = "{\"message\":\"404 Group Not Found\"}";
        [
            json!({"code": 1, "out": body, "err": "glab: 404 Group Not Found (HTTP 404)"}),
            json!({"code": 1, "out": body, "err": "", "status": 404}),
        ]
    }

    #[test]
    fn owned_falls_back_to_the_user_endpoint_on_a_group_404_and_answers_the_same_shape() {
        purlis_core::unsteered!();
        for missing in no_such_group() {
            let (backend, recorded) = over(json!([page(GROUP, missing), page(USER, personal())]));
            let owned = backend.owned(&caller(), &Owner::new("solo")).unwrap();
            assert_eq!(owned.len(), 1);
            assert_eq!(owned[0].name, "dots");
            assert_eq!(owned[0].path_with_namespace, "solo/dots");
            assert_eq!(owned[0].ssh_url, "git@gitlab.com:solo/dots.git");
            assert_eq!(owned[0].forge, Kind::GitLab);
            spent(&recorded);
        }
    }

    #[test]
    fn only_a_not_found_answer_falls_back() {
        purlis_core::unsteered!();
        let forbidden = json!({"code": 1, "out": "{\"message\":\"403 Forbidden\"}", "err": "",
                               "status": 403});
        let (backend, recorded) = over(json!([page(GROUP, forbidden)]));
        let err = backend.owned(&caller(), &Owner::new("solo")).unwrap_err();
        assert_eq!(err.failure(), &Failure::Forbidden, "{err}");
        assert!(err.said().contains("GitLab group 'solo'"), "{err}");
        spent(&recorded);
    }

    #[test]
    fn a_name_that_is_neither_a_group_nor_a_user_fails_in_the_user_endpoints_words() {
        purlis_core::unsteered!();
        let no_user = json!({"code": 1, "out": "{\"message\":\"404 User Not Found\"}",
                             "err": "", "status": 404});
        let [missing, _] = no_such_group();
        let (backend, recorded) = over(json!([page(GROUP, missing), page(USER, no_user)]));
        let err = backend.owned(&caller(), &Owner::new("solo")).unwrap_err();
        assert_eq!(err.failure(), &Failure::NotFound, "{err}");
        assert!(
            err.said()
                .contains("GitLab user 'solo' (no group has that name)")
                && err.said().contains("404 User Not Found"),
            "{err}"
        );
        spent(&recorded);
    }
}

/// GitLab's `owned` for a group lists the group's own projects and its subgroups', never a
/// project another namespace shared into it (#804). `groups/:id/projects` defaults
/// `with_shared` to `true` (GitLab 19.4 `doc/api/groups.md`); GitHub's organisation listing
/// has no such case, so purlis asks with `with_shared=false`. The recording answers only that
/// request, so a listing asked without it fails here.
mod gitlab_shared_projects {
    use super::*;

    #[test]
    fn a_project_shared_into_the_group_is_not_discovered_as_the_groups() {
        purlis_core::unsteered!();
        let own = json!([{"id": 1, "name": "Api", "path": "api", "path_with_namespace": "acme/api",
            "default_branch": "main", "description": null, "web_url": "https://gitlab.com/acme/api",
            "ssh_url_to_repo": "git@gitlab.com:acme/api.git", "topics": []}]);
        let text = json!({"source": "GitLab 19.4 REST API docs, groups.md: List projects \
                                     (with_shared defaults to true)",
            "exchanges": [{"call": {"endpoint": {"rest": {"method": null,
                "path": "groups/acme/projects?per_page=100&page=1&include_subgroups=true\
                         &archived=false&with_shared=false"}}, "fields": []},
                "reply": {"code": 0, "out": own.to_string()}}]});
        let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
        let backend = Forge::default_of(Kind::GitLab).backend_over(recorded.clone());
        let owned = backend.owned(&caller(), &Owner::new("acme")).unwrap();
        let paths: Vec<&str> = owned
            .iter()
            .map(|r| r.path_with_namespace.as_str())
            .collect();
        assert_eq!(paths, ["acme/api"], "only the group's own projects");
        spent(&recorded);
    }
}

/// FW-2a's and FW-2b's acceptance: the contract passes against the native client with no `gh`
/// or `glab` on `PATH`, on github.com, gitlab.com, a GitHub Enterprise Server and a self-managed
/// GitLab (FG-3). Every native case
/// runs again in a child of this binary whose environment is emptied and whose `PATH` is one
/// empty directory.
#[test]
fn the_native_contract_passes_with_no_forge_cli_on_path() {
    purlis_core::unsteered!();
    let empty = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let out = purlis_core::forklock::output(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "github_native::",
                "gitlab_native::",
                "github_self_managed::",
                "gitlab_self_managed::",
                "--test-threads=2",
            ])
            .env_clear()
            .env("PATH", empty.path())
            .env("HOME", home.path()),
    )
    .unwrap();
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "the native contract failed with no CLI on PATH:\n{said}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        said.contains(&format!("{} passed", 4 * CASES.len())),
        "the child did not run every native case:\n{said}"
    );
}

/// The live run reads its scene from each forge's own fields (`live.rs`): the two repos' ids, the
/// request open from the save branch and the one that landed. Here against answers shaped as
/// each forge's documentation shows them (GitHub REST `2022-11-28` repos and pulls; GitLab 19.4
/// `projects.md` and `merge_requests.md`).
mod the_live_scene {
    use super::*;

    fn answering(answers: Vec<(String, Value)>) -> impl Fn(&str) -> Result<Value, String> {
        move |path: &str| {
            answers
                .iter()
                .find(|(p, _)| p == path)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| format!("nothing answers {path}"))
        }
    }

    fn github(landed: Value) -> Vec<(String, Value)> {
        vec![
            ("repos/fixture/api".into(), json!({"id": 101, "full_name": "fixture/api"})),
            ("repos/fixture/web".into(), json!({"id": 102, "full_name": "fixture/web"})),
            (
                "repos/fixture/api/pulls?state=open&head=fixture:charter%2Fsave&base=main&per_page=1"
                    .into(),
                json!([{"number": 2, "state": "open", "head": {"ref": "charter/save", "sha": SHA}}]),
            ),
            (
                "repos/fixture/api/pulls?state=closed&head=fixture:charter%2Flanded&per_page=1"
                    .into(),
                landed,
            ),
        ]
    }

    #[test]
    fn github_names_the_open_request_its_head_and_the_merge_commit_of_the_one_that_landed() {
        purlis_core::unsteered!();
        let landed = json!([{"number": 1, "state": "closed",
                             "merged_at": "2026-10-04T00:00:00Z", "merge_commit_sha": MERGE}]);
        let get = answering(github(landed));
        let scene = live::discover(Kind::GitHub, "github.com", "fixture", &get).unwrap();
        assert_eq!(
            (
                scene.api_id.as_str(),
                scene.web_id.as_str(),
                scene.tree_id.as_str()
            ),
            ("101", "102", "101")
        );
        assert_eq!((scene.open, scene.head.as_str()), (2, SHA));
        assert_eq!((scene.merged, scene.merge.as_str()), (1, MERGE));
        assert_eq!(scene.opened_from, live::OPENED_FROM);
        assert_eq!(
            (scene.opened, scene.created.clone(), scene.queue),
            (None, None, false)
        );
        assert_eq!(
            scene.request_url(2),
            "https://github.com/fixture/api/pull/2"
        );
    }

    #[test]
    fn a_github_request_closed_without_merging_is_no_landed_request() {
        purlis_core::unsteered!();
        let closed = json!([{"number": 1, "state": "closed", "merged_at": null,
                             "merge_commit_sha": MERGE}]);
        let get = answering(github(closed));
        let refused = live::discover(Kind::GitHub, "github.com", "fixture", &get).unwrap_err();
        assert!(refused.contains("closed, not merged"), "{refused}");
    }

    #[test]
    fn gitlab_reads_iids_and_escapes_the_repo_as_one_segment() {
        purlis_core::unsteered!();
        let get = answering(vec![
            ("projects/fixture%2Fapi".into(), json!({"id": 201})),
            ("projects/fixture%2Fweb".into(), json!({"id": 202})),
            (
                "projects/fixture%2Fapi/merge_requests?state=opened&source_branch=charter%2Fsave&target_branch=main&per_page=1"
                    .into(),
                json!([{"id": 9001, "iid": 2, "sha": SHA}]),
            ),
            (
                "projects/fixture%2Fapi/merge_requests?state=merged&source_branch=charter%2Flanded&per_page=1"
                    .into(),
                json!([{"id": 9000, "iid": 1, "merge_commit_sha": MERGE}]),
            ),
        ]);
        let scene = live::discover(Kind::GitLab, "gitlab.com", "fixture", &get).unwrap();
        assert_eq!(
            (scene.api_id.as_str(), scene.web_id.as_str()),
            ("201", "202")
        );
        assert_eq!((scene.open, scene.head.as_str()), (2, SHA));
        assert_eq!((scene.merged, scene.merge.as_str()), (1, MERGE));
        assert_eq!(
            scene.issue_url(5),
            "https://gitlab.com/fixture/api/-/issues/5"
        );
    }

    /// The issues `create` leaves behind are found by the harness's own listing, by the label
    /// `create` gives them and the case's title, and closed by the numbers that listing names: never a pull request, never an issue
    /// with another title.
    #[test]
    fn the_sweep_closes_only_open_issues_with_the_cases_title() {
        purlis_core::unsteered!();
        let closed = std::cell::RefCell::new(Vec::new());
        let list = |path: &str| -> Result<Value, String> {
            assert_eq!(
                path, "repos/fixture/api/issues?state=open&labels=ws%3Aalpha&per_page=100",
                "by the label `create` puts on it"
            );
            Ok(json!([
                {"number": 7, "title": live::ISSUE_TITLE},
                {"number": 8, "title": "Something else"},
                {"number": 9, "title": live::ISSUE_TITLE, "pull_request": {"url": "x"}},
                {"number": 10, "title": live::ISSUE_TITLE},
            ]))
        };
        let close = |number: u64| -> Result<(), String> {
            closed.borrow_mut().push(number);
            Ok(())
        };
        let swept = live::sweep(Kind::GitHub, "repos/fixture/api", &list, &close).unwrap();
        assert_eq!(swept, [7, 10]);
        assert_eq!(*closed.borrow(), [7, 10]);

        let gitlab = |path: &str| -> Result<Value, String> {
            assert_eq!(
                path,
                "projects/fixture%2Fapi/issues?state=opened&labels=charter%3A%3Aws%3A%3Aalpha&per_page=100"
            );
            Ok(json!([{"id": 5001, "iid": 3, "title": live::ISSUE_TITLE}]))
        };
        let swept =
            live::sweep(Kind::GitLab, "projects/fixture%2Fapi", &gitlab, &|_| Ok(())).unwrap();
        assert_eq!(swept, [3], "by its iid, not its id");
    }

    #[test]
    fn a_fixture_with_no_open_request_says_which_one_it_lacks() {
        purlis_core::unsteered!();
        let mut answers = github(json!([]));
        answers[2].1 = json!([]);
        let refused =
            live::discover(Kind::GitHub, "github.com", "fixture", &answering(answers)).unwrap_err();
        assert!(
            refused.contains("request open from charter/save into main"),
            "{refused}"
        );
    }
}
