# RLIMIT_AS via preexec_fn is the wrong delivery on a dev-on-macOS tool: s

_2026-08-31 15:18 · persistent_

RLIMIT_AS via preexec_fn is the wrong delivery on a dev-on-macOS tool: setrlimit(RLIMIT_AS) RAISES on Darwin, and a preexec_fn that raises surfaces as subprocess.SubprocessError IN THE PARENT — so every child fails to start. preexec_fn is also documented-unsafe from a thread pool. Use /bin/sh -c 'ulimit -v $1 || exit N; shift; exec "$@"' instead: thread-safe, and exec keeps the pid the parent holds for killpg and timeouts. Probe once whether the platform enforces it (macOS does not) rather than wrapping blindly.
