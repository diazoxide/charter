# 0.56.0: _BODY_BUDGET has NO legal value — its two guards demand disjoint

_2026-09-04 10:32 · persistent_

0.56.0: _BODY_BUDGET has NO legal value — its two guards demand disjoint ranges. Floor (test_the_bound_is_an_exception_rather_than_the_normal_path) needs >= 111,724 so at most one GitHub-sized release stays reshaped; ceiling (test_the_staged_release_has_headroom) needs <= 106,250 = RELEASE_BODY_MAX 125,000 * HEADROOM 0.85. Filed as #878. Structural, not a one-off: the floor counts ALL history's releases that GitHub would take whole, so its demand ratchets up as releases land in the 106k-125k band while the ceiling stays a fixed fraction. The ceiling test SKIPS on a stamped tree (no unreleased entries), so a release branch sees only the floor and a raise there looks free. Do NOT raise it under release pressure to get green.
