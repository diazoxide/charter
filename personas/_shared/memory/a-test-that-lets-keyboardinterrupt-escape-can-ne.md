# A test that lets KeyboardInterrupt escape can never be measured by the d

_2026-09-12 21:04 · persistent_

A test that lets KeyboardInterrupt escape can never be measured by the deletion sweep. Found on PR 992 (harness profiles task 3, 2026-09-12): unittest reads a KeyboardInterrupt as the operator interrupting the run, so a mutant that raises one ends the run on SIGINT instead of failing — neither green nor red, and the sweep reports the line as unresolved rather than pinned. It came back unresolved twice before the cause was found. Catch it inside the test and the same mutation is an ordinary failure, so the line becomes measurable. Also from that PR: codex is not installed on the CI runner, so a test using the real shutil.which for it passes locally and fails in CI.
