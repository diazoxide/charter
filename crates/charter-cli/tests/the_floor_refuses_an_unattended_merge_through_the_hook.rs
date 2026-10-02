//! The release floor's merge half through the real `charter hook pretooluse`: each route by which
//! a forge CLI or git could merge a request is refused when the host says nobody is watching
//! (`bypassPermissions`) and is untouched in `default` mode. The read-only calls an unattended
//! run needs are answered with nothing at all.
//!
//! The full set of spellings is at the unit seam, in
//! `crates/charter-core/tests/an_unattended_run_cannot_merge_a_request_by_any_route.rs`; this
//! file proves the hook reaches that answer for every route.

use std::io::Write;
use std::process::{Command, Stdio};

/// A plane the guard's gated arms run in, and a home that is not the operator's.
struct Plane {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
    home: std::path::PathBuf,
}

impl Plane {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("a directory");
        let base = tmp.path().canonicalize().expect("a real path");
        let root = base.join("plane");
        let home = base.join("home");
        std::fs::create_dir_all(&root).expect("the plane");
        std::fs::create_dir_all(&home).expect("the home");
        std::fs::write(root.join("charter.toml"), "schema = 1\n").expect("the marker");
        Self {
            _tmp: tmp,
            root,
            home,
        }
    }

    /// The `permissionDecisionReason` the hook printed for `command` in `mode`, or `None`.
    fn denial(&self, command: &str, mode: &str) -> Option<String> {
        let payload = serde_json::json!({
            "session_id": "11111111-2222-4333-8444-555555555555",
            "cwd": ".",
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "permission_mode": mode,
            "tool_input": {"command": command},
        })
        .to_string();
        let mut child = Command::new(env!("CARGO_BIN_EXE_charter"))
            .args(["hook", "pretooluse"])
            .current_dir(&self.root)
            .env_clear()
            .env("CHARTER_ROOT", &self.root)
            .env("HOME", &self.home)
            .env("CHARTER_HARNESS", "claude-code")
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the hook runs");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(payload.as_bytes())
            .expect("the payload");
        let out = child.wait_with_output().expect("the hook finishes");
        assert_eq!(out.status.code(), Some(0), "{command}: {out:?}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let v: serde_json::Value = serde_json::from_str(&stdout).ok()?;
        let o = v.get("hookSpecificOutput")?;
        (o.get("permissionDecision")?.as_str()? == "deny").then(|| {
            o.get("permissionDecisionReason")?
                .as_str()
                .map(str::to_owned)
        })?
    }

    fn refused_unattended_only(&self, cmds: &[&str]) {
        for cmd in cmds {
            let reason = self
                .denial(cmd, "bypassPermissions")
                .unwrap_or_else(|| panic!("not refused unattended: {cmd}"));
            assert!(reason.contains("Re-run this step **attended**"), "{reason}");
            assert_eq!(self.denial(cmd, "default"), None, "attended: {cmd}");
        }
    }

    fn allowed_unattended(&self, cmds: &[&str]) {
        for cmd in cmds {
            assert_eq!(self.denial(cmd, "bypassPermissions"), None, "{cmd}");
        }
    }
}

#[test]
fn the_rest_merge_endpoint_is_refused_unattended() {
    Plane::new().refused_unattended_only(&[
        "gh api -X PUT repos/o/r/pulls/12/merge",
        "gh api repos/o/r/pulls/12/merge --method=put",
        "glab api -X PUT projects/1/merge_requests/5/merge",
    ]);
}

#[test]
fn branch_merge_and_merge_train_endpoints_are_refused_unattended() {
    Plane::new().refused_unattended_only(&[
        "gh api -X POST repos/o/r/merges -f base=main -f head=feature",
        "glab api -X POST projects/:id/merge_trains/merge_requests/5",
    ]);
}

#[test]
fn an_auto_merge_setting_in_a_request_update_is_refused_unattended() {
    Plane::new().refused_unattended_only(&[
        "glab api -X PUT projects/1/merge_requests/5 -f merge_when_pipeline_succeeds=true",
    ]);
}

#[test]
fn an_api_call_whose_effect_cannot_be_read_is_refused_unattended() {
    Plane::new().refused_unattended_only(&[
        "gh api -X PUT \"$EP\"",
        "glab api -X PUT projects/1/merge_requests/5 --input body.json",
    ]);
}

#[test]
fn a_graphql_merge_auto_merge_or_merge_queue_mutation_is_refused_unattended() {
    Plane::new().refused_unattended_only(&[
        "gh api graphql -f query='mutation { mergePullRequest(input: {pullRequestId: \"X\"}) { clientMutationId } }'",
        "gh api graphql -f query='mutation { enablePullRequestAutoMerge(input: {pullRequestId: \"X\"}) { clientMutationId } }'",
        "gh api graphql -f query='mutation { enqueuePullRequest(input: {pullRequestId: \"X\"}) { clientMutationId } }'",
        "glab api graphql -f query='mutation { mergeRequestAccept(input: {projectPath: \"g/p\", iid: \"5\"}) { errors } }'",
    ]);
}

#[test]
fn a_graphql_call_whose_query_cannot_be_read_is_refused_unattended() {
    Plane::new().refused_unattended_only(&[
        "gh api graphql -F query=@merge.graphql",
        "gh api graphql --input q.json",
        "gh api graphql -f query=\"$Q\"",
    ]);
}

#[test]
fn a_push_option_that_sets_auto_merge_is_refused_unattended() {
    Plane::new().refused_unattended_only(&[
        "git push -o merge_request.merge_when_pipeline_succeeds origin feat",
        "git push --push-option=merge_request.auto_merge origin feat",
        "git -c push.pushOption=merge_request.auto_merge push origin feat",
        "git config push.pushOption merge_request.auto_merge",
    ]);
}

#[test]
fn a_forge_cli_spelling_of_a_merge_is_refused_unattended() {
    Plane::new().refused_unattended_only(&[
        "gh pr -R o/r merge 12",
        "glab mr --repo g/p merge 5",
        "glab mr accept 5",
        "glab mr create --fill --yes --auto-merge",
        "gh alias set m 'pr merge --auto'",
        "gh alias import aliases.yml",
    ]);
}

#[test]
fn a_merge_inside_a_string_a_shell_runs_is_refused_unattended() {
    Plane::new().refused_unattended_only(&[
        "bash -c 'gh pr merge 12'",
        "sh -c \"gh api -X PUT repos/o/r/pulls/12/merge\"",
    ]);
}

#[test]
fn read_only_api_calls_and_ordinary_pushes_stay_allowed_unattended() {
    Plane::new().allowed_unattended(&[
        "gh api repos/o/r/pulls/12",
        "gh api repos/o/r/pulls/12/merge",
        "gh api repos/o/r/commits/abc/check-runs --jq '.check_runs[].conclusion'",
        "glab api projects/:id/merge_requests/5",
        "gh api graphql -f query='query($n: Int!) { repository(owner: \"o\", name: \"r\") { pullRequest(number: $n) { mergeable } } }' -F n=12",
        "gh api -X POST repos/o/r/issues/12/comments -f body=hi",
        "git push -o merge_request.create origin feat",
        "glab mr create --fill --yes",
    ]);
}

#[test]
fn a_git_alias_that_stands_for_a_publish_is_refused_unattended() {
    Plane::new().refused_unattended_only(&[
        "git -c alias.p='push -o merge_request.auto_merge' p origin feat",
        "git -c alias.t=tag t v1.0.0",
        "git config alias.p 'push -o merge_request.auto_merge'",
        "git config alias.x '!git push --tags'",
    ]);
}

#[test]
fn a_forge_alias_for_part_of_a_held_command_is_refused_unattended() {
    Plane::new().refused_unattended_only(&["gh alias set p pr", "glab alias set a api"]);
}

#[test]
fn a_write_whose_words_the_shell_expands_is_refused_unattended() {
    Plane::new().refused_unattended_only(&[
        "gh api -X PUT repos/o/r/pulls/12/m[e]rge",
        "gh api -X PUT repos/o/r/git/../pulls/12/merge",
        "gh api -X PUT",
        "gh api -X PATCH repos/o/r --input settings.json",
    ]);
}

#[test]
fn a_shell_script_is_read_past_its_options_and_through_stdin_unattended() {
    Plane::new().refused_unattended_only(&[
        "bash -c -- 'gh pr merge 12'",
        "bash <<< 'gh pr merge 12'",
        "echo 'gh pr merge 12' | bash",
    ]);
}

#[test]
fn a_graphql_query_the_shell_could_rewrite_is_refused_unattended() {
    Plane::new().refused_unattended_only(&[
        "gh api graphql -f query='query($M: ID) { $M }'",
        "gh api graphql/. -f query='mutation { mergePullRequest(input: {pullRequestId: \"X\"}) { clientMutationId } }'",
    ]);
}

#[test]
fn a_command_that_prints_the_forge_token_is_refused_unattended() {
    Plane::new().refused_unattended_only(&[
        "gh auth token",
        "gh auth token --hostname github.com",
        "gh auth status --show-token",
        "glab auth status -t",
        "glab config get token --host gitlab.com",
        "git credential fill",
        "gh alias set t 'auth token'",
    ]);
}

#[test]
fn reads_that_are_merely_named_merge_and_the_login_without_its_token_stay_allowed_unattended() {
    Plane::new().allowed_unattended(&[
        "gh api -X PATCH repos/o/r/git/refs/heads/merge-fix -f sha=abc",
        "gh auth status",
        "glab auth status",
        "glab config get editor",
        "git config alias.co checkout",
        "gh alias set co 'pr checkout'",
    ]);
}
