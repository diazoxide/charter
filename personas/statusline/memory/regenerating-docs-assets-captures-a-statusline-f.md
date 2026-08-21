# Regenerating docs/assets captures: a statusline feature that renders onl

_2026-08-21 11:03 · persistent_

Regenerating docs/assets captures: a statusline feature that renders only from live state (the ⚡N age running badge) shows up as NOTHING in a fresh capture unless docs/assets/demo-plane.sh writes the fixture for it. demo-plane.sh already fabricates inventory/repos.json, pieces/seen/*.json and the glstate cache in exactly the shape the real writer uses; an inflight record ({"agent":n,"ts":epoch} under STATE_DIR/dispatch-inflight, two of them since the count renders only above one) belongs in the same list. Add the fixture in the same change as any new live-state surface, or the next regeneration is silently half-stale.
