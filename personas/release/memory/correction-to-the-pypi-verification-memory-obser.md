# CORRECTION to the PyPI-verification memory (observed cutting 0.46.0, 202

_2026-08-19 17:11 · persistent_

CORRECTION to the PyPI-verification memory (observed cutting 0.46.0, 2026-08-19): gating on the simple index is NOT sufficient. 'curl -s https://pypi.org/simple/charter-cp/ | grep charter_cp-0.46.0-py3-none-any.whl' matched, AND the version endpoint answered 200 with both artifacts listed, and 'uv tool install --force --refresh charter-cp==0.46.0' STILL failed with 'no version of charter-cp==0.46.0 ... requirements are unsatisfiable'. There is a further lag after the wheel appears in the index. The reliable gate is the install itself: 'until uv tool install --force --refresh charter-cp==<X.Y.Z> >/tmp/ins.log 2>&1; do sleep 15; done'. It cleared within a couple of minutes. Do not report a release as verified on a 200 or on an index grep — only on a successful install plus 'charter version' showing the new number.
