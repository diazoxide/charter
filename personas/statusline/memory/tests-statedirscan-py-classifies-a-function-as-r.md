# tests/_statedirscan.py classifies a function as returning a STATE PATH f

_2026-09-12 17:20 · persistent_

tests/_statedirscan.py classifies a function as returning a STATE PATH from its return expression's text, so a one-line 'return frozenset(e.name for e in os.listdir(_state_dir()))' makes that function a state-path function even though it returns names. The taint then flows through every call site that passes its result onward and reports unrelated mkdir/write calls in the same module as unrouted (measured on charter/workspace.py: 4 mkdirs + 5 writes went red from adding one such reader). The fix is workspace.tab_order's shape: bind the listing to a local inside the try, and return an expression whose TEXT mentions no state call. Check with: python3 -c "from tests import _statedirscan as s; print(sorted(f for f in s.package_state_functions(s.load_package(), s.state_attribute_names()) if f.startswith('workspace.')))"
