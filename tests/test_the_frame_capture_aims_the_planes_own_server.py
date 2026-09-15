"""`docs/assets/capture-frame.sh` watches the server charter really starts the frame on.

**Ruling 46 moved it.** The capture launches a real frame inside a private `$TMUX_TMPDIR`
and then waits for its chats by asking the frame's own tmux server
(`wait_for_chats`, through `INNER`). That server was `charter` — one name every plane
shared — and is now the demo plane's own `charter-plane-<12 hex>`. A script still aimed at
`…/charter` asks a socket nothing starts: `--full` times out waiting for chats that are
open on the other server, and `cleanup` kills the server it was told about while the real
one keeps running.

Read rather than run: the script starts two tmux servers and a frame, which is a capture and
not a test. What is pinned is the two facts that decide which server it watches — it never
names the shared socket, and it asks production's own function for the name, so a change to
the hash cannot leave a second spelling behind here.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

_SCRIPT = Path(__file__).resolve().parents[1] / "docs" / "assets" / "capture-frame.sh"


class TheCaptureWatchesThePlanesOwnServer(unittest.TestCase):

    def setUp(self) -> None:
        self.text = _SCRIPT.read_text()
        self.code = "\n".join(line for line in self.text.splitlines()
                              if not line.lstrip().startswith("#"))

    def test_it_never_aims_at_the_shared_socket(self):
        self.assertIsNone(re.search(r'tmux-\$\(id -u\)/charter"', self.code),
                          "the capture still watches `charter`, which no frame starts on")

    def test_it_asks_charter_for_the_planes_own_server(self):
        self.assertIn("tmuxctl.plane_socket()", self.code)
        self.assertRegex(self.code, r'INNER="\$SOCKDIR/tmux-\$\(id -u\)/\$NAME"')

    def test_the_name_is_asked_in_the_plane_the_frame_launches_in(self):
        """The launch runs `cd '$PLANE' && exec charter frame …`; a name asked anywhere else
        could resolve another plane's state directory and so another server."""
        ask = self.code.index("tmuxctl.plane_socket()")
        self.assertIn('cd "$PLANE"', self.code[max(0, ask - 200):ask])
        self.assertLess(self.code.index('"$HERE/demo-plane.sh" "$PLANE"'), ask,
                        "the name is asked before the plane it names exists")


if __name__ == "__main__":
    unittest.main()
