# The [charter] version lock applies only UPGRADES unattended (#333, PR #3

_2026-08-22 09:31 · persistent_

The [charter] version lock applies only UPGRADES unattended (#333, PR #351). SessionStart installs a newer pin and only REPORTS an older one, because an upgrade can only add guards while a downgrade can only remove them, and SessionStart has no ask verdict — its output is context, whose only reader is a model. A deliberate pin-back is still 'charter version sync --cli'. The pin must also be an exact X.Y.Z: charter-cp==<pin> is a pip requirement specifier, and 'uv pip compile' resolves charter-cp==0.* to the latest 0.x.
