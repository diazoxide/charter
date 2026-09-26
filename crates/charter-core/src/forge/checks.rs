//! The checks at one exact commit, as one of five closed values (ADR 0060 §6; the Phase 4
//! spec §3.5).
//!
//! | value | means |
//! |---|---|
//! | `PASSED` | at least one check at this head, each concluded success, neutral or skipped |
//! | `FAILED` | one concluded failure, cancelled, timed out, startup failure or `action_required` |
//! | `RUNNING` | one is queued or in progress |
//! | `NOT RUN` | **zero** checks at this head; a stale run does not count |
//! | `UNKNOWN` | charter could not ask, or got an answer it does not recognise |
//!
//! Precedence is `UNKNOWN` > `FAILED` > `RUNNING` > `NOT RUN` > `PASSED`: the one value that
//! means "charter did not look" is never outranked by one that means "it looked and it was
//! fine". Nothing here reads the pull request's own roll-up (`gh pr checks`,
//! `mergeStateStatus`) or the status line's cached CI word: each turns "nothing ran" into
//! green (charter-plane #561).
//!
//! On GitHub the read is two: the check runs **and** the commit statuses at the sha, summed,
//! because CI that reports through statuses (Jenkins, Buildkite) has no check runs at all. On
//! GitLab it is the merge request's own pipelines at that sha. A merged-results pipeline runs
//! at a sha that is not the branch head, and charter cannot tie it to this head, so a merge
//! request whose only pipelines are at another sha is `UNKNOWN`, never `NOT RUN`.

use serde_json::Value;

use super::pr::Repo;
use super::{Kind, quote};

/// The five values, closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ci {
    // Declared in ascending precedence, so `max` is the value that decides.
    Passed,
    NotRun,
    Running,
    Failed,
    Unknown,
}

impl Ci {
    /// The word a row shows.
    pub fn word(self) -> &'static str {
        match self {
            Ci::Passed => "PASSED",
            Ci::Failed => "FAILED",
            Ci::Running => "RUNNING",
            Ci::NotRun => "NOT RUN",
            Ci::Unknown => "UNKNOWN",
        }
    }
}

/// The checks at one commit: how many were counted (`None` when charter could not ask), the
/// value, and why it is `UNKNOWN` when it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checks {
    pub total: Option<u64>,
    pub ci: Ci,
    pub why: Option<String>,
}

impl Checks {
    fn unknown(why: impl Into<String>) -> Checks {
        Checks {
            total: None,
            ci: Ci::Unknown,
            why: Some(why.into()),
        }
    }
}

/// What one run, status or pipeline says, before they are summed. `Uncounted` is a run the
/// forge itself disowned (`stale`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum One {
    Counted(Ci),
    Uncounted,
}

/// A GitHub check run: its `status` and `conclusion`.
fn check_run(run: &Value) -> One {
    let status = run["status"].as_str().unwrap_or("");
    if status != "completed" {
        return One::Counted(match status {
            "queued" | "in_progress" | "waiting" | "requested" | "pending" => Ci::Running,
            _ => Ci::Unknown,
        });
    }
    match run["conclusion"].as_str().unwrap_or("") {
        "success" | "neutral" | "skipped" => One::Counted(Ci::Passed),
        "failure" | "cancelled" | "timed_out" | "startup_failure" | "action_required" => {
            One::Counted(Ci::Failed)
        }
        "stale" => One::Uncounted,
        _ => One::Counted(Ci::Unknown),
    }
}

/// A GitHub commit status: its `state`.
fn commit_status(status: &Value) -> One {
    One::Counted(match status["state"].as_str().unwrap_or("") {
        "success" => Ci::Passed,
        "failure" | "error" => Ci::Failed,
        "pending" => Ci::Running,
        _ => Ci::Unknown,
    })
}

/// A GitLab pipeline: its `status`. `manual` waits on a person, so it did not pass, as
/// GitHub's `action_required` does not. A `skipped` pipeline ran nothing (`[ci skip]`, or
/// every job skipped), so it is not counted, and alone it is `NOT RUN` — unlike a GitHub check
/// run concluded `skipped`, which is one check among others saying it had nothing to do.
fn pipeline(p: &Value) -> One {
    if p["status"].as_str() == Some("skipped") {
        return One::Uncounted;
    }
    One::Counted(match p["status"].as_str().unwrap_or("") {
        "success" => Ci::Passed,
        "failed" | "canceled" | "manual" => Ci::Failed,
        "created" | "waiting_for_resource" | "preparing" | "pending" | "running" | "scheduled" => {
            Ci::Running
        }
        _ => Ci::Unknown,
    })
}

/// Sum what was read into one value. Zero counted is `NOT RUN`: silence is never a pass.
fn reduce(each: impl IntoIterator<Item = One>) -> Checks {
    let mut total = 0u64;
    let mut worst = Ci::Passed;
    for one in each {
        if let One::Counted(ci) = one {
            total += 1;
            worst = worst.max(ci);
        }
    }
    Checks {
        total: Some(total),
        ci: if total == 0 { Ci::NotRun } else { worst },
        why: None,
    }
}

/// Whether `sha` is a commit id charter will put in an API path: hex, 7 to 64.
pub fn sha_ok(sha: &str) -> bool {
    (7..=64).contains(&sha.len()) && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The checks at exactly `sha` of `repo`. `request` is the merge request's number, which
/// GitLab's read needs and GitHub's does not.
pub fn at(repo: &Repo, sha: &str, request: u64) -> Checks {
    if !sha_ok(sha) {
        return Checks::unknown(format!(
            "the forge named the head {}, which is not a commit id",
            crate::shown::short(sha)
        ));
    }
    let result = match repo.forge.kind {
        Kind::GitHub => github(repo, sha),
        Kind::GitLab => gitlab(repo, sha, request),
    };
    result.unwrap_or_else(Checks::unknown)
}

/// The list under `key`, read whole or refused: a page that does not hold every entry the
/// forge counted is a list charter could not enumerate, and that is `UNKNOWN`.
fn whole<'a>(answer: &'a Value, key: &str, what: &str) -> Result<&'a Vec<Value>, String> {
    let list = answer[key]
        .as_array()
        .ok_or_else(|| format!("the forge's {what} answer holds no '{key}' list"))?;
    let counted = answer["total_count"].as_u64().unwrap_or(list.len() as u64);
    if counted as usize != list.len() {
        return Err(format!(
            "the forge counted {counted} {what} and sent {}; charter reads them all or says it \
             could not",
            list.len()
        ));
    }
    Ok(list)
}

fn github(repo: &Repo, sha: &str) -> Result<Checks, String> {
    let (owner, name) = repo.owner_name();
    let base = format!("repos/{}/{}/commits/{sha}", quote(owner), quote(name));
    let runs = repo.ask(
        repo.api(None, &format!("{base}/check-runs?per_page=100"), &[]),
        &format!("reading the check runs at {sha} of {}", repo.path),
    )?;
    let statuses = repo.ask(
        repo.api(None, &format!("{base}/status?per_page=100"), &[]),
        &format!("reading the commit statuses at {sha} of {}", repo.path),
    )?;
    let runs = whole(&runs, "check_runs", "check runs")?;
    let statuses = whole(&statuses, "statuses", "commit statuses")?;
    Ok(reduce(
        runs.iter()
            .map(check_run)
            .chain(statuses.iter().map(commit_status)),
    ))
}

fn gitlab(repo: &Repo, sha: &str, request: u64) -> Result<Checks, String> {
    let answer = repo.ask(
        repo.api(
            None,
            &format!(
                "projects/{}/merge_requests/{request}/pipelines?per_page=100",
                quote(&repo.path)
            ),
            &[],
        ),
        &format!(
            "reading the pipelines of merge request !{request} of {}",
            repo.path
        ),
    )?;
    let pipelines = answer
        .as_array()
        .ok_or("the forge's pipelines answer is not a list")?;
    let here: Vec<&Value> = pipelines
        .iter()
        .filter(|p| p["sha"].as_str() == Some(sha))
        .collect();
    if here.is_empty() && !pipelines.is_empty() {
        return Err(format!(
            "merge request !{request} has pipelines, none of them at {sha} (a merged-results \
             pipeline runs at a commit that is not the branch head), so charter cannot say \
             what ran at this head"
        ));
    }
    // Newest first, as GitLab lists them: the latest pipeline at this head is its verdict.
    // `total` is then what was judged: that one pipeline, or none.
    Ok(reduce(here.first().map(|p| pipeline(p))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn runs(list: &[(&str, Option<&str>)]) -> Checks {
        reduce(list.iter().map(|(status, conclusion)| {
            check_run(&json!({"status": status, "conclusion": conclusion}))
        }))
    }

    #[test]
    fn zero_runs_is_not_run_and_never_passed() {
        let checks = runs(&[]);
        assert_eq!((checks.total, checks.ci), (Some(0), Ci::NotRun));
    }

    #[test]
    fn success_neutral_and_skipped_pass() {
        let checks = runs(&[
            ("completed", Some("success")),
            ("completed", Some("neutral")),
            ("completed", Some("skipped")),
        ]);
        assert_eq!((checks.total, checks.ci), (Some(3), Ci::Passed));
    }

    #[test]
    fn action_required_is_a_failure_because_it_waits_on_a_person() {
        assert_eq!(
            runs(&[("completed", Some("action_required"))]).ci,
            Ci::Failed
        );
        for c in ["failure", "cancelled", "timed_out", "startup_failure"] {
            assert_eq!(runs(&[("completed", Some(c))]).ci, Ci::Failed, "{c}");
        }
    }

    #[test]
    fn a_stale_run_is_not_counted_so_alone_it_is_not_run() {
        let checks = runs(&[("completed", Some("stale"))]);
        assert_eq!((checks.total, checks.ci), (Some(0), Ci::NotRun));
        let checks = runs(&[("completed", Some("stale")), ("completed", Some("success"))]);
        assert_eq!((checks.total, checks.ci), (Some(1), Ci::Passed));
    }

    #[test]
    fn a_queued_or_running_check_is_running() {
        for s in ["queued", "in_progress", "waiting", "requested", "pending"] {
            assert_eq!(
                runs(&[(s, None), ("completed", Some("success"))]).ci,
                Ci::Running
            );
        }
    }

    #[test]
    fn anything_unrecognised_is_unknown_and_never_passed() {
        assert_eq!(runs(&[("completed", Some("brand-new"))]).ci, Ci::Unknown);
        assert_eq!(runs(&[("teleporting", None)]).ci, Ci::Unknown);
    }

    #[test]
    fn precedence_is_unknown_failed_running_not_run_passed() {
        let all = runs(&[
            ("completed", Some("success")),
            ("in_progress", None),
            ("completed", Some("failure")),
            ("completed", Some("mystery")),
        ]);
        assert_eq!(all.ci, Ci::Unknown);
        let no_unknown = runs(&[
            ("completed", Some("success")),
            ("in_progress", None),
            ("completed", Some("failure")),
        ]);
        assert_eq!(no_unknown.ci, Ci::Failed);
        let running = runs(&[("completed", Some("success")), ("in_progress", None)]);
        assert_eq!(running.ci, Ci::Running);
    }

    #[test]
    fn commit_statuses_count_as_checks() {
        let checks = reduce(
            [json!({"state": "success"}), json!({"state": "pending"})]
                .iter()
                .map(commit_status),
        );
        assert_eq!((checks.total, checks.ci), (Some(2), Ci::Running));
        assert_eq!(
            reduce([json!({"state": "error"})].iter().map(commit_status)).ci,
            Ci::Failed
        );
    }

    #[test]
    fn a_gitlab_pipeline_waiting_on_a_person_did_not_pass() {
        assert_eq!(
            reduce([pipeline(&json!({"status": "manual"}))]).ci,
            Ci::Failed
        );
        assert_eq!(
            reduce([pipeline(&json!({"status": "success"}))]).ci,
            Ci::Passed
        );
        assert_eq!(
            reduce([pipeline(&json!({"status": "running"}))]).ci,
            Ci::Running
        );
    }

    #[test]
    fn a_skipped_gitlab_pipeline_ran_nothing_so_it_is_not_run() {
        let checks = reduce([pipeline(&json!({"status": "skipped"}))]);
        assert_eq!((checks.total, checks.ci), (Some(0), Ci::NotRun));
    }

    #[test]
    fn only_a_commit_id_reaches_an_api_path() {
        assert!(sha_ok("9f3a1c2"));
        assert!(sha_ok(&"a".repeat(40)));
        for bad in ["", "9f3a1c", "-X", "../../x", "9f3a1c2/..", &"a".repeat(65)] {
            assert!(!sha_ok(bad), "{bad}");
        }
    }

    #[test]
    fn no_word_for_silence_reads_as_passing() {
        for ci in [Ci::NotRun, Ci::Unknown] {
            assert!(!ci.word().contains("PASS"), "{}", ci.word());
        }
    }
}
