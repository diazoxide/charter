# Tests that call charter's real cmd_launch must derive from PersonaIso: c

_2026-08-23 20:36 · persistent_

Tests that call charter's real cmd_launch must derive from PersonaIso: cmd_launch's first act is state.reap, an rmtree over config.STATE_DIR/frame/, and a frame dir with no 'server' marker matches every server — so one unisolated test deletes the developer's live frame state. Faked tmux does not make a launcher test harmless. To find these, monkeypatch rmtree/Path.mkdir/os.replace/os.open(write) to raise on any path under the real .charter and run the whole suite; trace.record and menu.record swallow the exception, so record hits to a file rather than trusting a test failure.
