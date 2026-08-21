# Smoke-testing news probes in a scratch tree: news._is_checkout(d) requir

_2026-08-21 10:41 · persistent_

Smoke-testing news probes in a scratch tree: news._is_checkout(d) requires a pyproject.toml at d.parents[1], i.e. the TREE ROOT two levels above docs/news. A scratch copy of just charter/ + docs/news/ + charter.toml therefore has news._dir() == None and news.all() == [] — every probe silently passes and 'charter doctor' prints 'news: nothing to adopt' in under a second. That is indistinguishable from a working guard, and it made my first 0.47.1 counterfactual VACUOUS: the unguarded v0.47.0 news.py 'passed' only because it saw zero entries. Always assert the planted entry is in news.released() BEFORE concluding anything from a bounded run. Copy pyproject.toml into the scratch tree.
