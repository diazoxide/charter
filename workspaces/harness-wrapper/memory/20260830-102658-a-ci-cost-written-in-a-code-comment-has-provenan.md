# A CI cost written in a code comment has provenance in the job logs, not 

_2026-08-30 10:26 · persistent_

A CI cost written in a code comment has provenance in the job logs, not in the PR narrative: charter's sweep tool prints its own 'selection map: N source files in Xs', so nine runs of gh api repos/.../actions/jobs/ID/logs settled #670's 250-vs-350 s dispute empirically (242-285 s, so 350 matched no run). Re-measure from logs before trusting either copy of a duplicated figure.
