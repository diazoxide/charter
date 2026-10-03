//! The forge contract suite: every method of the forge seam's areas, run once per forge
//! against recorded exchanges (ADR 0070 §7, the recorded run; FG-3).
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
//! suite was written. FG-4's live nightly re-records them from github.com, gitlab.com and a
//! self-managed GitLab with `CHARTER_FORGE_BLESS=1`, and a PR that moves a recording says which
//! contract moved and why (ADR 0046).
//!
//! **Parity is checked, not trusted.** `every_method_of_the_seam_has_a_case_on_both_forges`
//! reads the area traits out of `src/forge/backend.rs` and fails when a method has no case here
//! or a case has no recording for one forge.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use charter_core::forge::backend::{About, Issues, NewWorkItem, Visibility};
use charter_core::forge::checks::{Checks, Ci};
use charter_core::forge::pr::{AutoMerge, MergeAs, MergedAt, Opened, Pr, Request, State};
use charter_core::forge::recorded::Recorded;
use charter_core::forge::{
    Caller, Capability, Forge, ForgeBackend, ForgeRef, Kind, Owner, Reach, RepoRecord, Support,
};
use serde_json::{Value, json};

#[path = "forge_contract/native.rs"]
mod native;

/// The head every recording's request is at.
const SHA: &str = "6dcb09b5b57875f334f61aebed695e2e4193db5e";
/// The commit a merged request landed as.
const MERGE: &str = "e5bd3914e2e596debea16f433f57875b5b90bcd6";

/// Every case, by the seam method it covers. The parity test compares this with the traits.
const CASES: [&str; 18] = [
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
}

/// A backend over a case's recording, the caller to ask as, and the check that every
/// recorded exchange was asked for.
pub struct Over {
    pub backend: Box<dyn ForgeBackend>,
    pub caller: Caller,
    /// The instance the backend is on.
    pub host: String,
    check: Box<dyn Fn()>,
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

/// A forge's backend over its recording of `case`, answered as `how` says.
fn over(kind: &str, case: &str, how: How) -> Over {
    let text = recording(kind, case);
    match how {
        How::Recorded => {
            let recorded = Arc::new(Recorded::parse(&text).unwrap());
            let forge = Forge::default_of(charter_core::forge::Kind::parse(kind).unwrap());
            let check = recorded.clone();
            Over {
                backend: forge.backend_over(recorded),
                caller: caller(),
                host: forge.host.clone(),
                check: Box::new(move || check.spent()),
            }
        }
        How::Native => native::over(kind, default_host(kind), &text),
        How::SelfManaged => native::over(kind, SELF_MANAGED, &text),
    }
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

    /// The record a recording's repo `name` under `acme` becomes, in neutral fields. GitLab
    /// names a repo by its `path` (`api`), not its display name (`Api`).
    fn acme(kind: &str, id: &str, name: &str, topics: &[&str]) -> RepoRecord {
        let host = match kind {
            "github" => "github.com",
            _ => "gitlab.com",
        };
        RepoRecord {
            id: Some(ForgeRef(id.into())),
            name: name.into(),
            path_with_namespace: format!("acme/{name}"),
            default_branch: Some("main".into()),
            description: String::new(),
            web_url: format!("https://{host}/acme/{name}"),
            ssh_url: format!("git@{host}:acme/{name}.git"),
            topics: topics.iter().map(|t| t.to_string()).collect(),
            forge: Kind::parse(kind).unwrap(),
        }
    }

    pub fn owned(kind: &str, how: How) {
        let recorded = over(kind, "owned", how);
        let backend = &recorded.backend;
        let repos = backend
            .owned(&recorded.caller, &Owner::new("acme"))
            .unwrap();
        assert_eq!(
            repos,
            [
                acme(kind, "1", "api", &[]),
                acme(kind, "2", "web", &["frontend"])
            ],
            "every field typed; a null description is empty"
        );
        spent(&recorded);
    }

    pub fn reachable(kind: &str, how: How) {
        let recorded = over(kind, "reachable", how);
        let backend = &recorded.backend;
        let repos = backend
            .reachable(&recorded.caller, &Owner::new("acme"))
            .unwrap();
        assert_eq!(
            repos,
            [acme(kind, "1", "api", &[])],
            "only the declared owner's repos"
        );
        spent(&recorded);
    }

    pub fn about(kind: &str, how: How) {
        let recorded = over(kind, "about", how);
        let backend = &recorded.backend;
        // Asked of the repo itself, as the inventory reads it: no forge id, which `about` never
        // needs, since both forges address the repo by its path.
        let repo = RepoRecord {
            id: None,
            ..acme(kind, "1", "api", &[])
        };
        assert_eq!(
            backend.about(&recorded.caller, &repo),
            Ok(About {
                visibility: Visibility::Private,
                issues: Issues::Open
            })
        );
        spent(&recorded);
    }

    pub fn top_level(kind: &str, how: How) {
        let recorded = over(kind, "top_level", how);
        let backend = &recorded.backend;
        let repo = acme(kind, "7", "api", &[]);
        assert_eq!(
            backend.top_level(&recorded.caller, &repo, None).unwrap(),
            ["Cargo.toml", "src"]
        );
        spent(&recorded);
    }

    pub fn open_or_update(kind: &str, how: How) {
        let recorded = over(kind, "open_or_update", how);
        let backend = &recorded.backend;
        let opened = backend
            .open_or_update(
                &recorded.caller,
                "acme/api",
                "charter/save",
                "main",
                "Save",
                "<!-- charter-save -->",
            )
            .unwrap();
        let url = match kind {
            "github" => "https://github.com/acme/api/pull/12",
            _ => "https://gitlab.com/acme/api/-/merge_requests/12",
        };
        assert_eq!(
            opened,
            Opened {
                pr: Pr {
                    number: 12,
                    url: url.into()
                },
                ours: true
            }
        );
        spent(&recorded);
    }

    pub fn state(kind: &str, how: How) {
        let recorded = over(kind, "state", how);
        let backend = &recorded.backend;
        let pr = Pr {
            number: 12,
            url: String::new(),
        };
        assert_eq!(
            backend.state(&recorded.caller, "acme/api", &pr),
            Ok(State::Merged {
                commit: Some(MERGE.into())
            })
        );
        spent(&recorded);
    }

    /// The body a request's description holds, as `charter change push` splices it.
    const BODY: &str = "Why this change.\n\n<!-- END charter change -->";

    pub fn body(kind: &str, how: How) {
        let recorded = over(kind, "body", how);
        let backend = &recorded.backend;
        let pr = Pr {
            number: 12,
            url: String::new(),
        };
        assert_eq!(
            backend.body(&recorded.caller, "acme/api", &pr),
            Ok(BODY.to_string())
        );
        spent(&recorded);
    }

    pub fn set_body(kind: &str, how: How) {
        let recorded = over(kind, "set_body", how);
        let backend = &recorded.backend;
        let pr = Pr {
            number: 12,
            url: String::new(),
        };
        assert_eq!(
            backend.set_body(&recorded.caller, "acme/api", &pr, BODY),
            Ok(())
        );
        spent(&recorded);
    }

    pub fn by_head(kind: &str, how: How) {
        let recorded = over(kind, "by_head", how);
        let backend = &recorded.backend;
        let found = backend
            .by_head(&recorded.caller, "acme/api", "charter/save")
            .unwrap()
            .expect("a request");
        assert_eq!(
            (found.number, found.state, found.head.as_str()),
            (12, State::Open, SHA)
        );
        let _: Request = found;
        spent(&recorded);
    }

    pub fn request_auto_merge(kind: &str, how: How) {
        let recorded = over(kind, "request_auto_merge", how);
        let backend = &recorded.backend;
        let pr = Pr {
            number: 12,
            url: String::new(),
        };
        assert_eq!(
            backend.request_auto_merge(&recorded.caller, "acme/api", &pr, SHA),
            Ok(AutoMerge::Queued)
        );
        spent(&recorded);
    }

    /// How every landing case asks to merge: a merge commit, carrying the trailer.
    fn merge_as(kind: &str) -> MergeAs {
        let sigil = charter_core::forge::Kind::parse(kind)
            .unwrap()
            .change_sigil();
        MergeAs {
            squash: false,
            title: format!("api-2: api ({sigil}12)"),
            message: "bump the api\n\nCharter-Change: api-2".to_string(),
        }
    }

    pub fn lands_through_queue(kind: &str, how: How) {
        let recorded = over(kind, "lands_through_queue", how);
        let pr = Pr {
            number: 12,
            url: String::new(),
        };
        assert_eq!(
            recorded
                .backend
                .lands_through_queue(&recorded.caller, "acme/api", &pr),
            Ok(true)
        );
        spent(&recorded);
    }

    pub fn merge_at(kind: &str, how: How) {
        let recorded = over(kind, "merge_at", how);
        let pr = Pr {
            number: 12,
            url: String::new(),
        };
        assert_eq!(
            recorded
                .backend
                .merge_at(&recorded.caller, "acme/api", &pr, SHA, &merge_as(kind)),
            Ok(MergedAt::Now)
        );
        spent(&recorded);
    }

    pub fn enqueue_at(kind: &str, how: How) {
        let recorded = over(kind, "enqueue_at", how);
        let pr = Pr {
            number: 12,
            url: String::new(),
        };
        assert_eq!(
            recorded
                .backend
                .enqueue_at(&recorded.caller, "acme/api", &pr, SHA, &merge_as(kind)),
            Ok(())
        );
        spent(&recorded);
    }

    pub fn checks_at(kind: &str, how: How) {
        let recorded = over(kind, "checks_at", how);
        let backend = &recorded.backend;
        assert_eq!(
            backend.checks_at(&recorded.caller, "acme/api", SHA, 12),
            Checks {
                total: Some(1),
                ci: Ci::Passed,
                why: None
            }
        );
        spent(&recorded);
    }

    pub fn open_on_branch(kind: &str, how: How) {
        let recorded = over(kind, "open_on_branch", how);
        let backend = &recorded.backend;
        assert_eq!(
            backend.open_on_branch(&recorded.caller, "acme/api", "charter/save"),
            Ok(Some(Value::from(12)))
        );
        spent(&recorded);
    }

    pub fn ci_word(kind: &str, how: How) {
        let recorded = over(kind, "ci_word", how);
        let backend = &recorded.backend;
        assert_eq!(
            backend.ci_word(&recorded.caller, "acme/api", "charter/save"),
            Ok(Some("success".to_string()))
        );
        spent(&recorded);
    }

    pub fn support(kind: &str, how: How) {
        let recorded = over(kind, "support", how);
        let backend = &recorded.backend;
        for what in Capability::ALL {
            let at = Reach::Repo("acme/api".into());
            assert_ne!(
                backend.support(&recorded.caller, &at, what),
                Support::Available,
                "{what:?}: what charter has not asked a repo about is never yes"
            );
        }
        // Epics, said of the instance: GitHub has none anywhere; gitlab.com has them (on a
        // Premium namespace, which a narrower reach would have to probe); a self-managed GitLab
        // has what its edition and licence have, which charter has not asked.
        let epics = backend.support(&recorded.caller, &Reach::Instance, Capability::Epics);
        // Sub-issues, said of the instance: github.com has them; a GitHub Enterprise Server has
        // what its version has, which charter has not asked.
        let sub_issues = backend.support(&recorded.caller, &Reach::Instance, Capability::SubIssues);
        match (kind, recorded.host.as_str()) {
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
        spent(&recorded);
    }

    pub fn create(kind: &str, how: How) {
        let recorded = over(kind, "create", how);
        let backend = &recorded.backend;
        let made = backend
            .create(
                &recorded.caller,
                "acme/api",
                &NewWorkItem {
                    title: "Port the picker".into(),
                    body: "What the todo said.\n\nSecond paragraph.".into(),
                    workspace_label: Some("alpha".into()),
                },
            )
            .unwrap();
        let (key, forge_ref, url) = match kind {
            "github" => (
                "github:github.com/acme/api#12",
                "I_kwDOAcme12",
                "https://github.com/acme/api/issues/12",
            ),
            _ => (
                "gitlab:gitlab.com/acme/api#12",
                "84012",
                "https://gitlab.com/acme/api/-/issues/12",
            ),
        };
        assert_eq!(made.key.as_str(), key);
        assert_eq!(made.forge_ref, Some(ForgeRef(forge_ref.into())));
        assert_eq!(made.url, url);
        spent(&recorded);
    }
}

/// One module per forge, holding every case of [`CASES`].
macro_rules! contract {
    ($module:ident, $forge:ident, $how:ident) => {
        mod $module {
            #[test]
            fn owned_lists_every_repo_of_the_owner_in_the_neutral_shape() {
                charter_core::unsteered!();
                super::cases::owned(stringify!($forge), super::How::$how);
            }
            #[test]
            fn reachable_keeps_only_the_declared_owners_repos() {
                charter_core::unsteered!();
                super::cases::reachable(stringify!($forge), super::How::$how);
            }
            #[test]
            fn top_level_lists_the_default_branchs_top_level_names() {
                charter_core::unsteered!();
                super::cases::top_level(stringify!($forge), super::How::$how);
            }
            #[test]
            fn open_or_update_opens_one_request_when_none_is_open() {
                charter_core::unsteered!();
                super::cases::open_or_update(stringify!($forge), super::How::$how);
            }
            #[test]
            fn state_reads_a_merged_request_with_the_commit_it_landed_as() {
                charter_core::unsteered!();
                super::cases::state(stringify!($forge), super::How::$how);
            }
            #[test]
            fn body_reads_the_requests_description_whole() {
                charter_core::unsteered!();
                super::cases::body(stringify!($forge), super::How::$how);
            }
            #[test]
            fn set_body_replaces_the_requests_description_and_nothing_else() {
                charter_core::unsteered!();
                super::cases::set_body(stringify!($forge), super::How::$how);
            }
            #[test]
            fn by_head_finds_the_request_from_the_branch_with_its_head_commit() {
                charter_core::unsteered!();
                super::cases::by_head(stringify!($forge), super::How::$how);
            }
            #[test]
            fn request_auto_merge_queues_the_merge_at_the_pushed_commit() {
                charter_core::unsteered!();
                super::cases::request_auto_merge(stringify!($forge), super::How::$how);
            }
            #[test]
            fn lands_through_queue_reads_whether_the_target_branch_has_a_queue() {
                charter_core::unsteered!();
                super::cases::lands_through_queue(stringify!($forge), super::How::$how);
            }
            #[test]
            fn merge_at_merges_now_only_at_the_head_charter_read() {
                charter_core::unsteered!();
                super::cases::merge_at(stringify!($forge), super::How::$how);
            }
            #[test]
            fn enqueue_at_puts_the_request_in_the_queue_only_at_the_head_charter_read() {
                charter_core::unsteered!();
                super::cases::enqueue_at(stringify!($forge), super::How::$how);
            }
            #[test]
            fn checks_at_counts_one_passing_check_at_the_head() {
                charter_core::unsteered!();
                super::cases::checks_at(stringify!($forge), super::How::$how);
            }
            #[test]
            fn open_on_branch_names_the_open_requests_number() {
                charter_core::unsteered!();
                super::cases::open_on_branch(stringify!($forge), super::How::$how);
            }
            #[test]
            fn ci_word_maps_the_forges_success_to_success() {
                charter_core::unsteered!();
                super::cases::ci_word(stringify!($forge), super::How::$how);
            }
            #[test]
            fn support_never_reads_silence_as_yes() {
                charter_core::unsteered!();
                super::cases::support(stringify!($forge), super::How::$how);
            }
            #[test]
            fn about_says_whether_the_repo_is_public_and_takes_issues() {
                charter_core::unsteered!();
                super::cases::about(stringify!($forge), super::How::$how);
            }
            #[test]
            fn create_opens_an_issue_and_names_it_by_its_tracker_key_with_its_forge_ref_beside() {
                charter_core::unsteered!();
                super::cases::create(stringify!($forge), super::How::$how);
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
    charter_core::unsteered!();
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
        let forge = Forge::default_of(charter_core::forge::Kind::GitLab);
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
        charter_core::unsteered!();
        assert_eq!(ci_word_of("waiting_for_callback"), Some("pending".into()));
        assert_eq!(checks_of("waiting_for_callback"), Ci::Running);
    }

    #[test]
    fn a_pipeline_being_canceled_reads_as_canceled_and_did_not_pass() {
        charter_core::unsteered!();
        assert_eq!(ci_word_of("canceling"), Some("canceled".into()));
        assert_eq!(checks_of("canceling"), Ci::Failed);
    }

    #[test]
    fn auto_merge_waits_on_a_pipeline_waiting_for_a_callback() {
        charter_core::unsteered!();
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
        let forge = Forge::default_of(charter_core::forge::Kind::GitLab);
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
        charter_core::unsteered!();
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
        charter_core::unsteered!();
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

    fn about_of(kind: charter_core::forge::Kind, path: &str, out: Value) -> About {
        let text = json!({"source": "GitHub REST 2022-11-28 repos.md; GitLab 19.4 projects.md",
            "exchanges": [{"call": {"endpoint": {"rest": {"method": null, "path": path}},
                                    "fields": []},
                           "reply": {"code": 0, "out": out.to_string()}}]});
        let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
        let backend = Forge::default_of(kind).backend_over(recorded.clone());
        let repo = charter_core::forge::RepoRecord {
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
        charter_core::unsteered!();
        let github = |out| about_of(charter_core::forge::Kind::GitHub, "repos/acme/api", out);
        assert_eq!(
            github(json!({"private": false, "has_issues": false})),
            About {
                visibility: Visibility::Public,
                issues: Issues::Off
            }
        );
        assert_eq!(
            github(json!({"visibility": "internal", "permissions": {"pull": false}})).issues,
            Issues::NoRight
        );
    }

    #[test]
    fn gitlab_says_no_right_for_members_only_issues_to_a_stranger() {
        charter_core::unsteered!();
        let gitlab = |out| {
            about_of(
                charter_core::forge::Kind::GitLab,
                "projects/acme%2Fapi",
                out,
            )
        };
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
                issues: Issues::Open
            }
        );
        let off = json!({"visibility": "private", "issues_access_level": "disabled"});
        assert_eq!(gitlab(off).issues, Issues::Off);
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

    fn paths(kind: &str) -> Vec<String> {
        CASES
            .iter()
            .flat_map(|case| {
                let file: Value = serde_json::from_str(&recording(kind, case)).unwrap();
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
        charter_core::unsteered!();
        for kind in ["github", "gitlab"] {
            let paths = paths(kind);
            // Exactly the REST calls the recordings hold, so a recording that lost one, or a
            // reader that skipped some, fails here rather than checking less.
            let expected = match kind {
                "github" => 15,
                _ => 20,
            };
            assert_eq!(paths.len(), expected, "{kind}: the recorded REST calls");
            for path in paths {
                let listed = charter_core::netlog::template(&path);
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
/// GitLab's counterpart is gap 1 of `docs/forges.md` (#803).
mod github_personal_account {
    use super::*;

    fn page(path: &str, reply: Value) -> Value {
        json!({"call": {"endpoint": {"rest": {"method": null, "path": path}}, "fields": []},
               "reply": reply})
    }

    #[test]
    fn owned_falls_back_to_the_user_endpoint_on_a_404_and_answers_the_same_shape() {
        charter_core::unsteered!();
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

/// FW-2a's and FW-2b's acceptance: the contract passes against the native client with no `gh`
/// or `glab` on `PATH`, on github.com, gitlab.com, a GitHub Enterprise Server and a self-managed
/// GitLab (FG-3). Every native case
/// runs again in a child of this binary whose environment is emptied and whose `PATH` is one
/// empty directory.
#[test]
fn the_native_contract_passes_with_no_forge_cli_on_path() {
    charter_core::unsteered!();
    let empty = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let out = charter_core::forklock::output(
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
