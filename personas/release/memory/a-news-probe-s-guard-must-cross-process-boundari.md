# A news probe's guard must cross process boundaries AND come back: news._

_2026-08-21 12:17 · persistent_

A news probe's guard must cross process boundaries AND come back: news._dispatch marks CHARTER_NEWS_PROBE=<pid>:<markpath> for the length of a probe, so a spawned charter refuses to probe, and a refused descendant touches <markpath> so the outer probe withholds its exit code (#314/PR318). Marker liveness is asked with os.kill(pid,0) on POSIX ONLY — on Windows os.kill maps to TerminateProcess and would kill whatever the marker names.
