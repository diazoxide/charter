# Regenerating docs/assets captures: the render's ACTIVE PERSONA is as ses

_2026-08-19 10:00 · persistent_

Regenerating docs/assets captures: the render's ACTIVE PERSONA is as session-fragile as the workspace. persona._resolved precedence is --persona -> $CHARTER_PERSONA -> session pointer -> terminal pointer -> .charter/active-persona -> charter.toml [persona] default -> personas/.default. 'charter persona use X' writes only a per-SESSION pointer, so a capture taken by the shell that built the demo plane shows X active while anyone else regenerating falls through to charter.toml — which 'charter init' now seeds with the scaffolded 'steward'. Pin it with 'charter persona default X' (writes charter.toml) the way demo-plane.sh already pins [workspace] default.
