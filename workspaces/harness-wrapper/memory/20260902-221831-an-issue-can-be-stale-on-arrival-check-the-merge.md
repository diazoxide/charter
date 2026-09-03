# An issue can be STALE ON ARRIVAL: check the merge log before believing i

_2026-09-02 22:18 · persistent_

An issue can be STALE ON ARRIVAL: check the merge log before believing its 'the suite cannot redden this' claim. #828 (filed 18:51 +04) said _capture_transcript's text.encode('utf-8','replace') was 'a survivor the suite cannot redden' and should be deleted once the decode was settled. #820 (0ac694b, merged 17:44 +04 — one hour EARLIER) had already added ACaptureCharterCannotEncodeStillLands, which drives that exact handler with a surrogate-bearing capture through the class's own stub seam. Deleting it would have cut a pinned, documented floor. This is trap 4 in the flesh: unreachable in production and observable in the suite are different things — grep for a dependant before cutting, and date-check the issue against git log.
