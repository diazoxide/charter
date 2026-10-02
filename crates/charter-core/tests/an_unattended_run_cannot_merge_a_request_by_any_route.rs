//! The release floor's merge half, at the public seam: [`floorguard::release_floor_reason`].
//!
//! An unattended run may open a pull or merge request and may not merge one. That rule is about
//! the ACT, so every way a forge CLI or git can perform the act is the same command to the floor:
//! a merge endpoint, an auto-merge or merge-queue setting, and a call whose effect on a merge
//! endpoint cannot be read off the command line are all refused unattended. Attended, every one
//! of them is untouched, and the read-only calls an unattended run needs stay allowed.
//!
//! `crates/charter-cli/tests/the_floor_refuses_an_unattended_merge_through_the_hook.rs` puts the
//! same command lines through the real `charter hook pretooluse`.

use charter_core::floorguard::release_floor_reason;

/// Each command is refused unattended and allowed attended.
fn refused_unattended_only(cmds: &[&str]) {
    for cmd in cmds {
        let said = release_floor_reason(cmd, true);
        assert!(said.is_some(), "not refused unattended: {cmd}");
        assert_eq!(release_floor_reason(cmd, false), None, "attended: {cmd}");
    }
}

/// Each command is allowed unattended.
fn allowed_unattended(cmds: &[&str]) {
    for cmd in cmds {
        assert_eq!(release_floor_reason(cmd, true), None, "{cmd}");
    }
}

#[test]
fn the_rest_merge_endpoint_is_refused_in_any_method_spelling_or_flag_order() {
    charter_core::unsteered!();
    refused_unattended_only(&[
        "gh api -X PUT repos/o/r/pulls/12/merge",
        "gh api repos/o/r/pulls/12/merge -X PUT",
        "gh api -XPUT repos/o/r/pulls/12/merge",
        "gh api --method PUT repos/o/r/pulls/12/merge",
        "gh api --method=put /repos/o/r/pulls/12/merge -f merge_method=squash",
        "gh api -X=PUT repos/{owner}/{repo}/pulls/12/merge",
        "gh api -iX PUT repos/o/r/pulls/12/merge",
        "gh api repos/o/r/pulls/12/merge -f sha=abc",
        "gh api --hostname ghe.example -X PUT https://ghe.example/api/v3/repos/o/r/pulls/12/merge",
        "gh api -X PUT repos/o/r/pulls/12/merge?x=1",
        "gh api -X PUT repos/o/r/pulls/12/%6Derge",
        "gh api -X PUT repos/o/r/pulls/12/MERGE/",
        "gh api -H 'Accept: application/json' -X PUT -- repos/o/r/pulls/12/merge",
        "glab api -X PUT projects/1/merge_requests/5/merge",
        "glab api --method PUT projects/group%2Fproj/merge_requests/5/merge",
        "glab api projects/:id/merge_requests/5/merge -X PUT -F squash=true",
        "glab api -X PUT https://gitlab.example/api/v4/projects/1/merge_requests/5/merge",
        "cd /tmp && gh api -X PUT repos/o/r/pulls/12/merge",
        "FOO=1 gh api -X PUT repos/o/r/pulls/12/merge",
    ]);
}

#[test]
fn branch_merge_and_merge_train_endpoints_are_refused() {
    charter_core::unsteered!();
    refused_unattended_only(&[
        "gh api -X POST repos/o/r/merges -f base=main -f head=feature",
        "gh api repos/o/r/merges -f base=main -f head=feature",
        "gh api -X POST repos/o/r/merge-upstream -f branch=main",
        "glab api -X POST projects/:id/merge_trains/merge_requests/5",
        "glab api -X POST projects/1/merge_requests/5/cancel_merge_when_pipeline_succeeds",
    ]);
}

#[test]
fn an_auto_merge_setting_in_a_request_update_is_refused() {
    charter_core::unsteered!();
    refused_unattended_only(&[
        "glab api -X PUT projects/1/merge_requests/5 -f merge_when_pipeline_succeeds=true",
        "glab api -X PUT projects/1/merge_requests/5 -F auto_merge=true",
        "glab api -X PUT projects/1/merge_requests/5 --field=auto_merge=true",
        "gh api -X PATCH repos/o/r -F allow_auto_merge=true",
    ]);
}

#[test]
fn an_api_call_whose_effect_on_a_merge_endpoint_cannot_be_read_is_refused() {
    charter_core::unsteered!();
    refused_unattended_only(&[
        // The endpoint, or the method, is the shell's to fill in.
        "gh api -X PUT \"$EP\"",
        "gh api -X PUT \"repos/o/r/pulls/$N/$WHAT\" ",
        "gh api -X \"$M\" repos/o/r/pulls/12/merge",
        "gh api \"$EP\" -f sha=abc",
        "glab api -X PUT `cat ep`",
        // The body is a file or stdin, on a request.
        "glab api -X PUT projects/1/merge_requests/5 --input body.json",
        "gh api -X PUT repos/o/r/pulls/12/merge --input -",
        // A flag the reader does not know could be anything.
        "gh api --frobnicate PUT repos/o/r/pulls/12/merge",
    ]);
}

#[test]
fn a_graphql_merge_auto_merge_or_merge_queue_mutation_is_refused() {
    charter_core::unsteered!();
    refused_unattended_only(&[
        "gh api graphql -f query='mutation { mergePullRequest(input: {pullRequestId: \"X\"}) { clientMutationId } }'",
        "gh api graphql -f query='mutation($id: ID!) { enablePullRequestAutoMerge(input: {pullRequestId: $id}) { clientMutationId } }' -f id=X",
        "gh api graphql -f query='mutation { enqueuePullRequest(input: {pullRequestId: \"X\"}) { clientMutationId } }'",
        "gh api graphql -f query='mutation { mergeBranch(input: {repositoryId: \"R\", base: \"main\", head: \"f\"}) { clientMutationId } }'",
        "gh api graphql --raw-field 'query=mutation M { mergePullRequest(input: {pullRequestId: \"X\"}) { clientMutationId } }'",
        "gh api -X POST graphql -F query='mutation { mergePullRequest(input: {pullRequestId: \"X\"}) { clientMutationId } }'",
        "gh api /graphql -f query='mutation { mergePullRequest(input: {pullRequestId: \"X\"}) { clientMutationId } }'",
        "glab api graphql -f query='mutation { mergeRequestAccept(input: {projectPath: \"g/p\", iid: \"5\"}) { errors } }'",
        "glab api graphql -f query='mutation { mergeRequestSetAutoMerge(input: {projectPath: \"g/p\", iid: \"5\"}) { errors } }'",
    ]);
}

#[test]
fn a_graphql_call_whose_query_cannot_be_read_is_refused() {
    charter_core::unsteered!();
    refused_unattended_only(&[
        "gh api graphql -F query=@merge.graphql",
        "gh api graphql --input q.json",
        "gh api graphql --input - <<'EOF'\n{\"query\":\"mutation { mergePullRequest(input: {}) { clientMutationId } }\"}\nEOF",
        "gh api graphql -f query=\"$Q\"",
        "gh api graphql -f query='query { $REST }'",
        "gh api graphql -f query=\"$(cat q.graphql)\"",
        "gh api graphql -f id=X",
    ]);
}

#[test]
fn a_push_option_that_sets_auto_merge_is_refused() {
    charter_core::unsteered!();
    refused_unattended_only(&[
        "git push -o merge_request.merge_when_pipeline_succeeds origin feat",
        "git push -o merge_request.create -o merge_request.auto_merge origin feat",
        "git push --push-option=merge_request.auto_merge origin feat",
        "git push --push-option merge_request.merge_when_pipeline_succeeds origin feat",
        "git push origin feat -omerge_request.auto_merge",
        "git push -uo merge_request.auto_merge origin feat",
        "git push -o \"$OPT\" origin feat",
        "git -C /repo push -o merge_request.auto_merge origin feat",
    ]);
}

#[test]
fn a_push_option_that_sets_auto_merge_through_git_config_is_refused() {
    charter_core::unsteered!();
    refused_unattended_only(&[
        "git -c push.pushOption=merge_request.auto_merge push origin feat",
        "git -c push.pushoption=merge_request.merge_when_pipeline_succeeds push",
        "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=push.pushOption GIT_CONFIG_VALUE_0=merge_request.auto_merge git push origin feat",
        "GIT_CONFIG_PARAMETERS=\"'push.pushOption'='merge_request.auto_merge'\" git push",
        "git config push.pushOption merge_request.auto_merge",
        "git config --add push.pushOption merge_request.merge_when_pipeline_succeeds",
        "git config set push.pushOption merge_request.auto_merge",
    ]);
}

#[test]
fn a_repository_flag_before_the_verb_does_not_hide_a_merge() {
    charter_core::unsteered!();
    refused_unattended_only(&[
        "gh pr -R o/r merge 12",
        "gh pr --repo o/r merge 12 --auto",
        "gh -R o/r pr merge 12",
        "glab mr -R g/p merge 5",
        "glab mr --repo g/p merge 5",
        "gh release -R o/r create v1",
    ]);
}

#[test]
fn a_verbs_own_alias_is_the_same_verb() {
    charter_core::unsteered!();
    refused_unattended_only(&["glab mr accept 5", "gh release new v1"]);
}

#[test]
fn opening_a_request_with_auto_merge_set_is_refused() {
    charter_core::unsteered!();
    refused_unattended_only(&[
        "glab mr create --fill --yes --auto-merge",
        "glab mr create --auto-merge=true --fill",
        "glab mr new --fill --auto-merge",
    ]);
}

#[test]
fn an_alias_that_would_merge_is_refused_when_it_is_made() {
    charter_core::unsteered!();
    refused_unattended_only(&[
        "gh alias set m 'pr merge --auto'",
        "gh alias set m 'api -X PUT repos/o/r/pulls/$1/merge'",
        "gh alias set --shell m 'gh pr merge \"$1\"'",
        "gh alias set m '!gh pr merge $1'",
        "gh alias set m -",
        "gh alias set m \"$EXP\"",
        "gh alias import aliases.yml",
        "glab alias set m 'mr merge'",
        "glab alias set m 'mr accept'",
    ]);
}

#[test]
fn a_merge_inside_a_string_a_shell_runs_is_refused() {
    charter_core::unsteered!();
    refused_unattended_only(&[
        "bash -c 'gh pr merge 12'",
        "sh -c \"gh api -X PUT repos/o/r/pulls/12/merge\"",
        "bash -lc 'git push -o merge_request.auto_merge origin feat'",
        "eval gh pr merge 12",
        "zsh -c 'gh release create v1'",
    ]);
    allowed_unattended(&["bash -c 'gh pr view 12'", "sh -c 'git push origin feat'"]);
}

#[test]
fn read_only_api_calls_stay_allowed_unattended() {
    charter_core::unsteered!();
    allowed_unattended(&[
        "gh api repos/o/r/pulls/12",
        "gh api repos/o/r/pulls/12/merge",
        "gh api -X GET repos/o/r/pulls/12/merge",
        "gh api repos/o/r/commits/abc/check-runs --jq '.check_runs[].conclusion'",
        "gh api -X GET repos/o/r/pulls -f state=open",
        "gh api --paginate repos/o/r/pulls/12/files -q '.[].filename'",
        "gh api \"repos/o/r/pulls/$N\"",
        "gh api repos/o/merge-tool/pulls",
        "glab api projects/:id/merge_requests/5",
        "glab api projects/:id/merge_requests/5/pipelines",
        "glab api projects/:id/merge_trains",
        "gh api graphql -f query='query($n: Int!) { repository(owner: \"o\", name: \"r\") { pullRequest(number: $n) { mergeable mergeStateStatus autoMergeRequest { enabledAt } } } }' -F n=12",
        "gh api graphql -f query='{ viewer { login } }'",
        "glab api graphql -f query='query { project(fullPath: \"g/p\") { mergeRequest(iid: \"5\") { mergeStatus } } }'",
    ]);
}

#[test]
fn ordinary_writes_pushes_and_requests_stay_allowed_unattended() {
    charter_core::unsteered!();
    allowed_unattended(&[
        "gh api -X POST repos/o/r/issues/12/comments -f body=hi",
        "gh api -X POST repos/o/merge-tool/issues -f title=x",
        "gh api -X PATCH repos/o/r/pulls/12 -f title=x",
        "glab api -X POST projects/1/merge_requests -f source_branch=a -f target_branch=main -f title=x",
        "glab api -X POST projects/1/merge_requests/5/notes -f body=hi",
        "git push -o ci.skip origin feat",
        "git push -o merge_request.create -o merge_request.target=main origin feat",
        "git push --push-option=merge_request.title=x origin feat",
        "git push origin feat",
        "git -c push.pushOption=ci.skip push origin feat",
        "git config push.pushOption ci.skip",
        "git config --get push.pushOption",
        "gh pr -R o/r view 12",
        "gh pr create --title merge --body x",
        "glab mr create --fill --yes",
        "glab mr create --auto-merge=false --fill",
        "glab mr --repo g/p view 5",
        "gh alias set co 'pr checkout'",
        "gh alias list",
    ]);
}
