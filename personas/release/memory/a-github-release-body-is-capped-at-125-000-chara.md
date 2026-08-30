# A GitHub Release body is capped at 125,000 characters and gh forwards th

_2026-08-30 02:16 · persistent_

A GitHub Release body is capped at 125,000 characters and gh forwards the refusal rather than truncating. release.yml's announce job is needs: publish, so a body over the cap fails AFTER the irreversible PyPI upload, and the workflow_dispatch retry cannot repair it — publish re-runs without skip-existing and PyPI rejects a version it already has. Measure len(news.render_body(version)) before tagging; 0.52.0 published at 111,349.
