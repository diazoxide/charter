# Regenerating docs/assets captures on a machine with git commit signing:

_2026-08-22 17:23 · persistent_

Regenerating docs/assets captures on a machine with git commit signing: demo-plane.sh's fixture repos pin user.name/user.email but (until 0.48.0 recapture) not signing, so an operator with commit.gpgsign=true + a 1Password/gpg signer gets 'failed to write commit object' on the first fixture commit and set -e kills the plane build halfway — the capture appears to 'not work here' when it is only inheriting the operator's git. Fix is per-repo 'git config commit.gpgsign false' in mk(), matching charter's own token-only/unsigned repo policy (charter git-policy). Never fix it by unsetting global config for the run; the fixture must be self-contained.
