# A HEAD SHA WITH NO CHECKS AND A SHA THE REPO DOES NOT HAVE ARE DIFFERENT

_2026-08-29 12:02 · persistent_

A HEAD SHA WITH NO CHECKS AND A SHA THE REPO DOES NOT HAVE ARE DIFFERENT, AND ONLY ONE ENDPOINT CAN TELL THEM APART. Measured against github.com on 2026-08-29 while splitting #644: for a sha the repo does not have, 'commits/<sha>/check-runs' answers HTTP 422, while the combined status endpoint answers 200 with total_count: 0 — indistinguishable from a real head that simply has no checks yet. Trusting the status endpoint alone reports 'no checks ran' for a commit that does not exist. Requiring BOTH reads to succeed is what separates them. Related: gh's statusCheckRollup is null at a head with zero check runs, which parses to the same None as a rate-limited call — that is #561's mechanism, confirmed live. Both are the same shape: an ABSENCE standing in for a VALUE.
