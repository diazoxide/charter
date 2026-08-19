# The simple-index poll is necessary but NOT sufficient: at 0.44.1 curl sa

_2026-08-18 23:53 · persistent_

The simple-index poll is necessary but NOT sufficient: at 0.44.1 curl saw charter_cp-0.44.1-py3-none-any.whl in https://pypi.org/simple/charter-cp/, and the very next 'uv tool install --force --refresh charter-cp==0.44.1' STILL failed 'requirements are unsatisfiable'; a retry ~20s later succeeded. uv and curl hit different CDN edges, so --refresh cannot fix it. Wrap the pinned install in a retry loop (attempt, sleep 20, re-attempt) rather than treating one failure as a broken publish. A failed pinned install is harmless — it leaves the OLD CLI in place, which is why 'charter --version' after the loop is the only proof that matters.
