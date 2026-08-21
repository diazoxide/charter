# #336 boundary correction: .charter/ sits UNDER the plane ROOT, so 'refus

_2026-08-22 00:53 · persistent_

#336 boundary correction: .charter/ sits UNDER the plane ROOT, so 'refuse a path whose realpath leaves the plane' does NOT stop persona.md -> ../../.charter/vaults/x.json. Containment for plane-data reads is drawn at the DATA roots: personas/, workspaces/, PERSONA_STATE_DIR (the last is in because ephemeral memory lives inside .charter/ and must keep working).
