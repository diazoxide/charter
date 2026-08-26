# overlay
PR: https://github.com/diazoxide/charter/pull/554
Branch: phase2-task2-overlay-surface

## Unpinned guards

### 1

overlay.close_argvs's refusal guard is unpinned. Deleting `if arm is None or not tmuxctl.PANE_ID_RE.fullmatch(overlay_pane): return []` from /Users/aharon/IdeaProjects/charter/charter/frame/overlay.py left the FULL suite green (5937, OK). With it gone, close_argvs(SOCKET, harness="%0", overlay_pane="") emits `kill-pane -t ""` — and the module docstring's own last paragraph records measuring on tmux 3.7c that an empty kill target kills the pane the command is running against. The one measurement the module leads with is undefended at the one call site that can produce it. tests/test_frame_overlay.py:508 (test_closing_hands_the_pane_back_and_disarms_the_hatch) only ever passes %0/%7; no test in the repo calls close_argvs with a refused id.

### 2

overlay.modal_argvs's refusal guard is unpinned. Deleting `if arm is None: return []` left the FULL suite green (5937, OK). The docstring states the property in words: "An empty list when either id is not tmux's own word for a pane: charter would rather open no overlay than open one it cannot promise a way out of." Without the guard the function returns [None, select-pane %7, resize-pane -Z %7] — charter selects and zooms a modal overlay whose hatch was never armed, which is exactly the state the escape hatch exists for. Both tests that call modal_argvs (tests/test_frame_overlay.py:161 and :171) pass %0/%7 only.

### 3

close_argvs's ORDER is unpinned. Swapping to `kill-pane` before `select-pane` left the FULL suite green (5937, OK). The docstring calls the order the property ("The same order the hatch itself runs in"). The identical claim IS pinned for hatch_command (test_returning_to_the_harness_comes_first went RED) and for modal_argvs (test_the_hatch_is_armed_before_the_surface_can_capture_anything went RED) — only the close path's own test uses `any(...)` and never asserts order. Measured on a real tmux 3.7c with harness=%0, panel=%1, zoomed overlay=%2: shipped order lands focus on %0; swapped, the kill lands focus on the PANEL %1 first.

### 4

conf_text's "the hatch bind goes LAST" ordering is unpinned. Moving `overlay.hatch_bind()` from the end of the list to the first line left the FULL suite green (5937, OK). The new docstring in /Users/aharon/IdeaProjects/charter/charter/commands_frame.py states this is the branch's whole compatibility story below tmuxctl.FLOOR: "it is why the line goes LAST: whatever a tmux too old to parse it does with the rest of the file, everything charter needs has already been applied by the time it gets there." tests/test_frame_overlay.py:483 asserts only membership (`assertIn(overlay.hatch_bind(), text.split("\n"))`), which is position-blind.

### 5

Surface.move's "never wraps" is unpinned, and the consequence is worse than the docstring's. Replacing the clamp with `(self._sel + delta) % len(self.rows)` left the FULL suite green (5937, OK). Because `home` and `end` are spelled `move(-len(self.rows))` and `move(+len(self.rows))` in Surface.handle, modulo turns both into NO-OPS: verified, Home from row 5 of 10 leaves the selection on row 5 (shipped: 0), End leaves it on 5 (shipped: 9). Home and End stop working entirely and nothing notices.

### 6

LEAVE's cursor restore is unpinned. Dropping `\x1b[?25h` from LEAVE left the FULL suite green (5937, OK) — with ENTER still writing `\x1b[?25l`, the overlay hands the pane back with the cursor hidden, which is the constant's own docstring verbatim: "an overlay that raised must not leave the operator on an alternate screen with no cursor, which is a terminal that looks broken and takes a `reset` to fix." Dropping `\x1b[?25l` from ENTER is also green. test_what_was_on_the_pane_comes_back and test_the_pane_is_restored_even_when_the_paint_raises pin the 1049 pair and not the cursor.

### 7

MOUSE_ON's deliberate choice of 1000 over 1002/1003 is unpinned. Changing MOUSE_ON to `\x1b[?1006h\x1b[?1003h` left the FULL suite green (5937, OK). The constant's docstring says "deliberately NOT 1002/1003, which add motion, because §4f closed the event kinds without `drag`" — and this module keeps no press state by design, so a motion-reporting request is precisely the event flood it states it cannot handle. Reversing MOUSE_OFF's withdrawal order (documented "in the reverse order it was asked for") is also green.

### 8

_CHROME_ROWS is unpinned. Setting `_CHROME_ROWS = _HEADER_ROWS` (footer dropped) left the FULL suite green (5937, OK) — verified non-equivalent: Surface._window(height=6) returns (0, 4) shipped and (0, 5) mutated, and Surface.handle's page size changes with it. The constant's docstring is explicit about why it is named once: "two answers to 'where does row 0 start' is an off-by-one nobody sees until a click selects the wrong thing." Nothing sees it. `_FOOTER_ROWS = 0` and `_GAP = 0` are also green at full suite.

### 9

_MIN_TITLE's floor is unpinned, and the test that exists for the property is measured at a width where the floor cannot bind. Changing _title_width to `min(longest, room // 2)` left the FULL suite green (5937, OK); so does `_MIN_TITLE = 0`. Verified the floor only binds below ~20 columns: at width 12 shipped returns 8 and the mutant returns 4; at width 34 — the width tests/test_frame_overlay.py:347 (test_a_narrow_pane_still_has_a_note_column) renders at — both return 15. The constant's docstring calls this "the floor the cap below stops at rather than a value it trades away"; no test renders narrow enough to reach it.

### 10

_title_width sizes the title column from tui.width, and swapping it for len left the FULL suite green (5937, OK). Verified non-equivalent: for a CJK title, tui.width is 18 where len is 9, so the column would be sized to half the cells the title needs and every such title truncates. _MARK's own comment makes exactly this argument ("`tui.width` on the marker rather than `len`") for the two-character marker; the titles, which are the values that actually vary, get no test.

### 11

_MARK's equal-width invariant is unpinned. Changing it to ("> ", " ") left the FULL suite green (5937, OK), despite the constant's docstring: "Both entries are the same width by construction, so a selection moving does not move the text beside it." Moving the selection now shifts every row's text one column.

### 12

Surface.heading is not contained-before-drawn in any test. Removing `contain.one_line` from the heading in Surface.render left the FULL suite green (5937, OK). Verified non-equivalent: shipped renders a heading of "one\ntwo" as the escaped `one\x0atwo` on one line; the mutant emits a real newline into the single _paint write, splitting the pane's layout. The `heading` attribute's docstring says it is "Contained before it is drawn: a picker's title is a workspace or persona name in Task 6, which is a committed value", and the RowsAreContainedBeforeTheyAreMeasured class covers title and note only.

### 13

decode's "drop the introducer" branch is reachable and entirely untested. I replaced `buf = buf[2:]; continue` with a fall-through that raises TypeError on the very next line, and the FULL suite stayed green (5937, OK). Reachable input verified by hand: decode(b"\x1b[12", final=True) takes that branch and currently types `1` and `2` as keys. The branch carries its own safety rationale in a comment ("rather than announcing an Escape keypress, which is the one input that means 'leave now'") and nothing exercises it.

### 14

decode's Ctrl-C handling is unpinned. Removing the `elif ch == b"\x03"` branch left the FULL suite green (5937, OK). With it gone `\x03` falls to the printable filter and is dropped, so Ctrl-C on a modal overlay does nothing at all — against the branch's own comment: "Ctrl-C reads as 'leave', not as a signal: the surface has the tty in raw mode, so nothing else is going to turn this into one."

### 15

decode's two byte-level filters are unpinned. Removing the `text.isprintable()` guard left the FULL suite green (5937, OK) — every unhandled control byte becomes a key event. Replacing the `except UnicodeDecodeError: continue` with `ch.decode(errors="replace")` is also green — a partial UTF-8 byte becomes a U+FFFD keypress, against the comment on that very line ("a partial UTF-8 byte: not a keypress").

### 16

decode's wheel test is unpinned. Changing `if button & 64:` to `if button in (64, 65):` left the FULL suite green (5937, OK). Verified non-equivalent: an SGR report with button 68 (shift+wheel) is a `scroll` shipped and becomes a `click` mutated — so a modifier-wheel would select a row under the pointer instead of scrolling, on a surface whose docstring says a click only ever selects because presses arrive unpaired.

### 17

open_argv's environment-carrying is entirely unexercised. Deleting `*layout._env_argv(env)` left the FULL suite green (5937, OK). No test in the repo passes `env=` to open_argv at all — I confirmed by grepping every call site. The refusal path inside it is untested too: I triggered it by hand and layout._env_argv RAISES ValueError for a non-carriable name, which is the opposite of the None-not-a-raise contract the neighbouring arm_hatch_argv docstring states for the same launch context. Separately, dropping `-v` from the split-window argv (the overlay pane becomes a horizontal split) is also green at full suite.

### 18

_SPLIT_ROWS is unpinned. Changing 5 to 500 left the FULL suite green (5937, OK), although the constant's docstring names exactly one way this number can cost anything: "small enough that tmux never refuses the split for want of room in a short frame, which is the one way this number could cost anything." Nothing asserts the value or the property.

