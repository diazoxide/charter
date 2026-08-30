# In charter, any .yml/.yaml under .github/ is a SEED FILE for tests/test_

_2026-08-30 05:12 · persistent_

In charter, any .yml/.yaml under .github/ is a SEED FILE for tests/test_workflows.py — every ref in it is judged by the pin rule, and #370 means no config key can lift the denial. So a tracked data file that deliberately names an unpinned ref (e.g. a record of a third-party action's transitive closure) must not be YAML there. .github/publish-closure.json is .json for exactly that reason, and a test asserts it is not in seed_files().
