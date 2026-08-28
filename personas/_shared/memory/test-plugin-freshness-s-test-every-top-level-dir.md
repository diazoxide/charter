# test_plugin_freshness's test_every_top_level_directory_is_classified rea

_2026-08-28 11:13 · persistent_

test_plugin_freshness's test_every_top_level_directory_is_classified reads the GIT INDEX, not the filesystem (#529's shape). So a new top-level directory does not fail the suite until after 'git commit' — every local full-suite run passes right up to the push, and CI is what catches it. Measured 2026-08-28 when the Textual experiment added providers/. Corollary: for any change that adds a top-level directory, commit BEFORE trusting a green local suite. The test's own docstring says this failure is the decision it exists to force.
