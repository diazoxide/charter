# macOS HAS NO 'timeout' COMMAND, AND THE FAILURE IS A FALSE GREEN. 'timeo

_2026-09-02 23:58 · persistent_

macOS HAS NO 'timeout' COMMAND, AND THE FAILURE IS A FALSE GREEN. 'timeout 60 python3 -m unittest ...' on macOS exits 0 with 'command not found: timeout' — indistinguishable from a passing suite if you only check the exit code, and it prints no test trailer to notice the absence of. It fooled the release agent once on 0.55.0, and it fooled ME earlier the same day (I ran 'timeout 60 charter claude ...' and read rc=0 as success). Rule: never wrap a verification run in 'timeout' on macOS; run it bare and read the actual trailer ('Ran N tests ... OK'), not the exit code alone. Same class as [[local-green-can-lie-because-the-test-loader-diff]] — the harness around the suite lied, not the suite. gtimeout exists only if coreutils is installed; do not assume it.
