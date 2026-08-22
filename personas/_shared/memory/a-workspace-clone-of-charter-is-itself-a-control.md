# A workspace clone of charter is itself a control plane, so a concurrent

_2026-08-22 17:59 · persistent_

A workspace clone of charter is itself a control plane, so a concurrent 'charter save' can check out main and commit in that clone while you are working in it — it happened mid-task and moved HEAD off the feature branch. Assert branch + HEAD identity around any long operation in such a clone, and re-check after 'git stash pop'.
