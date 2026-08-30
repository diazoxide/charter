# The retry ref is a real choice: --ref v<X.Y.Z> retries the exact publish

_2026-08-30 10:31 · persistent_

The retry ref is a real choice: --ref v<X.Y.Z> retries the exact published tree and stays available forever (use it for transient failures); --ref main is only for when the fix is on main, and works only until main's pyproject.toml bumps past the version, because guard compares -f version= against the tree it is handed. Re-pushing a deleted tag is NOT a retry — it arrives as push and is refused at the upload.
