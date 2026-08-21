# Forge-data security audit (surface 2): filed #323 (gh -F @-filename via 

_2026-08-21 17:53 · persistent_

Forge-data security audit (surface 2): filed #323 (gh -F @-filename via branch name/remote path), #324 (no timeout + inherited stdin + 120s re-spawn), #325 (repo name -> filesystem path, no validation), #326 (control chars in rendered forge data). Fixed ONLY #323 in PR #327 (branch forge-argv-encoding, commit ad885d0): -F -> -f in GitHubForge.ci_status, plus encoding for github.list_repos owner and gitlab repo_tree/repo_tree_strict id. Suite 2910 -> 2914.
