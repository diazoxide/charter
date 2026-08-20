# A statusline surface that DELETES the record it draws from turns the mos

_2026-08-20 14:41 · persistent_

A statusline surface that DELETES the record it draws from turns the most alarming state into the blank one — 'presumed dead' and 'never happened' render identically (#308, inflight TTL). One retention number cannot do both jobs: split it into a *flag* threshold (keep the record, mark it) and a much later *prune* horizon. Corollary: once a record outlives the flag threshold, every consumer that RETIRES one by age (inflight.finish takes the oldest) must skip flagged records first, or a finishing peer deletes the stuck one and leaves a false live one behind.
