# real-plane-and-filesystem
PR: https://github.com/diazoxide/charter/pull/543 — OPEN, not merged. Two commits: 57c4bce (the guard, the production fix, #532) and cadd97a (the two frame-only spawn sites). Body lists `Closes #527` and `Closes #532`. No version bump, no stamping, no tag. No subagents dispatched.

Note: origin/main advanced by three `charter save` commits (ef92358, 4b79f7c, 6ed138f) while I worked; they touch only docs/superpowers/ and do not overlap my files. My branch is based on e1744f8. `git diff --name-status e1744f8..HEAD` touches exactly the 20 intended files and nothing else.

The eight sites the guard found once armed: test_cli_smoke.CliSmokeTest (ran `charter doctor` against the developer's personas/workspaces/vault registry, while its own docstring claimed isolation — $CHARTER_HOME covers the per-human directory and reads as though it covered the plane), test_docs_show.TestDocsCli (`charter docs`, which regenerates a plane's topology), test_packaging, test_dev_channel.VersionAlwaysPrints (`--version` reads the plane's build label, so the assertion depended on the operator's own `[update] channel` — #459's shape across a fork where RealPlaneRead cannot follow), test_statusline_gauge.ContextGaugeCase (the file already had the stub — in ONE case and not in the three beside it; moved to setUp), test_the_state_directory_is_charters_to_choose, test_self_relaunch_shadowing.WithADecoyCwd.
Branch: real-plane-and-filesystem

## Blocking

### 1

The five environments do not agree. Measured by me on the branch: cleared/fresh OK, cleared/used-plane OK, live frame FAILED(15 failures), CHARTER_WORKSPACE=anything FAILED(8), live frame + $CHARTER_ROOT FAILED(23). A green suite still means 'I happened to run it outside a frame'. The failures are pre-existing and byte-identical to origin/main by test name (env 4 diff empty; env 3 differs only by one flaky tmux-integration case) and belong to the ambient cluster #519/#521/#528, but the phase gate is on the suite's answer, not on which group owns it.

### 2

The spawn guard recognises argv spellings, not the class of 'spawn a charter'. Five spawns written into the suite and run against the guard's own _REAL_ROOT: [A] [python,'-m','charter','--version'] -> REFUSED; [B] [python,'-c','from charter import config; print(config.ROOT)'] -> RAN, and the child printed the REAL plane; [C] ['/bin/sh','-c','<python> -m charter --version'] -> RAN 'charter 0.53.0' against the real plane; [D] [python,'<plane>/charter/__main__.py','--version'] -> unexamined; [E] the same command as a shell=True string -> RAN. _charter_argv's own docstring justifies its second spelling by naming hooks/hooks.json, but every command in that file is a shell command STRING ('charter workspace _reconcile >/dev/null 2>&1', 'out="$(charter doctor 2>&1)" || ...') -- precisely the form the guard declines to parse, so the stated reason for the spelling points at a form the guard cannot see.

### 3

The [B] shape is already live in the suite, not hypothetical: my Popen recorder logged 9 charter-importing `python -c` children per run (test_a_deny_survives_a_broken_channel, test_util_run_stdin) whose plane, by the guard's own _child_plane criterion, IS the real one -- and every one is waved through. They happen to call config.use() on the next statement, so nothing is harmed today, but nothing enforces it, and 'a module-level charter import in a child that resolves its own plane' is exactly the shape that produced #527.

