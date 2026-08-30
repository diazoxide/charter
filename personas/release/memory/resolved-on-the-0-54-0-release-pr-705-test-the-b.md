# Resolved on the 0.54.0 release PR (#705): test_the_bound_is_an_exception

_2026-08-30 21:17 · persistent_

Resolved on the 0.54.0 release PR (#705): test_the_bound_is_an_exception_rather_than_the_normal_path now counts only releases GitHub would have accepted whole, not every elided release, so a legitimately huge release no longer reddens it. _BODY_BUDGET stays 100,000. Do not expect that failure on future release PRs. The finding worth keeping: test_the_staged_release_has_headroom reads render_body(UNRELEASED) and SKIPS when nothing is staged, which is every release commit after 'charter news stamp' — so the pin from above is silent on exactly the commit where raising _BODY_BUDGET would decide a published body, and a skipped test reports success. If anyone proposes raising it on a stamped tree, measure by hand.
