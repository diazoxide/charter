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

use charter_core::forge::checks::{Checks, Ci};
use charter_core::forge::pr::{AutoMerge, Opened, Pr, Request, State};
use charter_core::forge::recorded::Recorded;
use charter_core::forge::{Caller, Forge, ForgeBackend};
use serde_json::{Value, json};

/// The head every recording's request is at.
const SHA: &str = "6dcb09b5b57875f334f61aebed695e2e4193db5e";
/// The commit a merged request landed as.
const MERGE: &str = "e5bd3914e2e596debea16f433f57875b5b90bcd6";

/// Every case, by the seam method it covers. The parity test compares this with the traits.
const CASES: [&str; 10] = [
    "owned",
    "reachable",
    "top_level",
    "open_or_update",
    "state",
    "by_head",
    "request_auto_merge",
    "checks_at",
    "open_on_branch",
    "ci_word",
];

fn recordings() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/forge_contract")
}

/// A forge's backend over its recording of `case`, and the recording to check afterwards.
fn over(kind: &str, case: &str) -> (Box<dyn ForgeBackend>, Arc<Recorded>) {
    let file = recordings().join(kind).join(format!("{case}.json"));
    let text = std::fs::read_to_string(&file)
        .unwrap_or_else(|e| panic!("the recording {}: {e}", file.display()));
    let recorded = Arc::new(Recorded::parse(&text).unwrap());
    let forge = Forge::default_of(charter_core::forge::Kind::parse(kind).unwrap());
    (forge.backend_over(recorded.clone()), recorded)
}

/// Every recorded exchange was asked for.
fn spent(recorded: &Recorded) {
    assert_eq!(recorded.unspent(), Vec::new(), "recorded and never asked");
}

fn caller() -> Caller {
    Caller::command()
}

mod cases {
    use super::*;

    pub fn owned(kind: &str) {
        let (backend, recorded) = over(kind, "owned");
        let repos = backend.owned(&caller(), "acme").unwrap();
        let shown: Vec<(&str, &str, &str)> = repos
            .iter()
            .map(|r| {
                (
                    r["name"].as_str().unwrap(),
                    r["path_with_namespace"].as_str().unwrap(),
                    r["forge"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [("api", "acme/api", kind), ("web", "acme/web", kind)]
        );
        assert_eq!(repos[0]["default_branch"], "main");
        assert_eq!(repos[0]["description"], "", "a null description is empty");
        spent(&recorded);
    }

    pub fn reachable(kind: &str) {
        let (backend, recorded) = over(kind, "reachable");
        let repos = backend.reachable(&caller(), "acme").unwrap();
        let paths: Vec<&str> = repos
            .iter()
            .map(|r| r["path_with_namespace"].as_str().unwrap())
            .collect();
        assert_eq!(paths, ["acme/api"], "only the declared owner's repos");
        spent(&recorded);
    }

    pub fn top_level(kind: &str) {
        let (backend, recorded) = over(kind, "top_level");
        let repo = json!({"id": 7, "path_with_namespace": "acme/api", "default_branch": "main"});
        assert_eq!(
            backend.top_level(&caller(), &repo, None).unwrap(),
            ["Cargo.toml", "src"]
        );
        spent(&recorded);
    }

    pub fn open_or_update(kind: &str) {
        let (backend, recorded) = over(kind, "open_or_update");
        let opened = backend
            .open_or_update(
                &caller(),
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

    pub fn state(kind: &str) {
        let (backend, recorded) = over(kind, "state");
        let pr = Pr {
            number: 12,
            url: String::new(),
        };
        assert_eq!(
            backend.state(&caller(), "acme/api", &pr),
            Ok(State::Merged {
                commit: Some(MERGE.into())
            })
        );
        spent(&recorded);
    }

    pub fn by_head(kind: &str) {
        let (backend, recorded) = over(kind, "by_head");
        let found = backend
            .by_head(&caller(), "acme/api", "charter/save")
            .unwrap()
            .expect("a request");
        assert_eq!(
            (found.number, found.state, found.head.as_str()),
            (12, State::Open, SHA)
        );
        let _: Request = found;
        spent(&recorded);
    }

    pub fn request_auto_merge(kind: &str) {
        let (backend, recorded) = over(kind, "request_auto_merge");
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

    pub fn checks_at(kind: &str) {
        let (backend, recorded) = over(kind, "checks_at");
        assert_eq!(
            backend.checks_at(&caller(), "acme/api", SHA, 12),
            Checks {
                total: Some(1),
                ci: Ci::Passed,
                why: None
            }
        );
        spent(&recorded);
    }

    pub fn open_on_branch(kind: &str) {
        let (backend, recorded) = over(kind, "open_on_branch");
        assert_eq!(
            backend.open_on_branch(&caller(), "acme/api", "charter/save"),
            Ok(Some(Value::from(12)))
        );
        spent(&recorded);
    }

    pub fn ci_word(kind: &str) {
        let (backend, recorded) = over(kind, "ci_word");
        assert_eq!(
            backend.ci_word(&caller(), "acme/api", "charter/save"),
            Ok(Some("success".to_string()))
        );
        spent(&recorded);
    }
}

/// One module per forge, holding every case of [`CASES`].
macro_rules! contract {
    ($forge:ident) => {
        mod $forge {
            #[test]
            fn owned_lists_every_repo_of_the_owner_in_the_neutral_shape() {
                charter_core::unsteered!();
                super::cases::owned(stringify!($forge));
            }
            #[test]
            fn reachable_keeps_only_the_declared_owners_repos() {
                charter_core::unsteered!();
                super::cases::reachable(stringify!($forge));
            }
            #[test]
            fn top_level_lists_the_default_branchs_top_level_names() {
                charter_core::unsteered!();
                super::cases::top_level(stringify!($forge));
            }
            #[test]
            fn open_or_update_opens_one_request_when_none_is_open() {
                charter_core::unsteered!();
                super::cases::open_or_update(stringify!($forge));
            }
            #[test]
            fn state_reads_a_merged_request_with_the_commit_it_landed_as() {
                charter_core::unsteered!();
                super::cases::state(stringify!($forge));
            }
            #[test]
            fn by_head_finds_the_request_from_the_branch_with_its_head_commit() {
                charter_core::unsteered!();
                super::cases::by_head(stringify!($forge));
            }
            #[test]
            fn request_auto_merge_queues_the_merge_at_the_pushed_commit() {
                charter_core::unsteered!();
                super::cases::request_auto_merge(stringify!($forge));
            }
            #[test]
            fn checks_at_counts_one_passing_check_at_the_head() {
                charter_core::unsteered!();
                super::cases::checks_at(stringify!($forge));
            }
            #[test]
            fn open_on_branch_names_the_open_requests_number() {
                charter_core::unsteered!();
                super::cases::open_on_branch(stringify!($forge));
            }
            #[test]
            fn ci_word_maps_the_forges_success_to_success() {
                charter_core::unsteered!();
                super::cases::ci_word(stringify!($forge));
            }
        }
    };
}

contract!(github);
contract!(gitlab);

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
    let mut methods: Vec<String> = ["Repos", "Requests"]
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
            me.matches(&format!("contract!({kind});")).count(),
            1,
            "{kind} is instantiated once"
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
