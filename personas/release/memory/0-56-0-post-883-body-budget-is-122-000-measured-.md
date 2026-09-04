# 0.56.0 (post-#883): _BODY_BUDGET is 122_000 measured by news.sent_length

_2026-09-04 12:29 · persistent_

0.56.0 (post-#883): _BODY_BUDGET is 122_000 measured by news.sent_length = max(len(chars), len(utf8 bytes)); RESERVE=3_000 replaced HEADROOM=0.85, and CEILING is derived once so the floor and ceiling guards read ONE number — test_the_floor_can_never_ask_for_more_than_the_ceiling_allows now asserts they cannot cross (that was #878). 0.56.0's 23 entries render WHOLE at 121,271 bytes: only 729 bytes of headroom, and news._elision's own notice is 443 bytes. A release body has room for no further notes once it is that close — adding one restarts elision and can clear RELEASE_BODY_MAX (125,000) too. Each release renders only its own entries, so a tight body does NOT carry over to the next version.
