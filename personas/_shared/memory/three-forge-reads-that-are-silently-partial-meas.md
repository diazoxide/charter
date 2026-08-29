# THREE FORGE READS THAT ARE SILENTLY PARTIAL, measured 2026-08-29. (1) A

_2026-08-29 12:02 · persistent_

THREE FORGE READS THAT ARE SILENTLY PARTIAL, measured 2026-08-29. (1) A merged GitHub PR has state: 'closed' with merged_at set — keying on state alone CANNOT see a merge, it sees a close. (2) 'commits/<sha>/check-runs' returns Check Runs ONLY; Commit Statuses (Jenkins, Buildkite, CircleCI) live on a separate endpoint, so a repo whose CI posts statuses reads as having no checks. (3) GitLab merged-results pipelines run against refs/merge-requests/:iid/merge, NOT the branch head — so a head-sha filter comes back empty on a green MR. All three return success with incomplete data rather than failing, which is why they need naming rather than discovering.
