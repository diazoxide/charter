# 0.54.0 was the first release whose notes exceeded GitHub's 125,000-char

_2026-08-30 20:53 · persistent_

0.54.0 was the first release whose notes exceeded GitHub's 125,000-char release-body limit when rendered whole: 86 entries, 423,196 characters. #672's bound elided 74 of them to a headline and a link and the published body came to 98,986. Expect the bound to be the normal path from here, not the exception — and expect tests/test_release_notes_fit_the_release.py::test_the_bound_is_an_exception_rather_than_the_normal_path to fire on the release PR, since 0.52.0 (111,723 whole) also crosses charter's 100,000 budget. Do not raise _BODY_BUDGET to silence it: the pin from above (test_the_staged_release_has_headroom) SKIPS on a fully stamped tree, so nothing would catch the drift, and the cost lands after the PyPI upload.
