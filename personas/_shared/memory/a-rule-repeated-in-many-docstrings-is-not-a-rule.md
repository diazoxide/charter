# A rule repeated in many docstrings is not a rule the code keeps: charter

_2026-09-08 12:59 · persistent_

A rule repeated in many docstrings is not a rule the code keeps: charter said 'the palette's cursor starts on the first row that can run' in six places (leave.open_rows, commands_frame._palette_catalogue and ._as_a_drawer, tabmenu.catalogue, docs/frame.md, the 0.56.0 note) while Palette._refilter did self._sel = 0. Six guards were reasoning from a false premise. When a docstring states an invariant, grep for the code that enforces it before building on it (#931).
