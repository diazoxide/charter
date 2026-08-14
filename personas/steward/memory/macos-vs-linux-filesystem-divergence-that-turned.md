# macOS vs Linux filesystem divergence that turned CI red in this repo: pa

_2026-08-14 12:01 · persistent_

macOS vs Linux filesystem divergence that turned CI red in this repo: pathlib does NOT swallow EACCES (only ENOENT/ENOTDIR/EBADF/ELOOP), so Path.is_dir() and Path.glob() on an unreadable directory RAISE on Linux and answer False/[] on macOS. Any local-only test of an unreadable path is untested on CI's platform — patch the raising behaviour in explicitly.
