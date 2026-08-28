"""M10 — the third component shape, refuted.

The shape neither component in this package uses, and the one that looks like the obvious
compromise: `render` starts a real (non-headless) Textual app on a background thread and
returns `[]` immediately, so charter's loop keeps running and keeps handing the component
fresh snapshots while Textual owns the pixels.

It does not work, it fails silently, and both halves are worth having measured.

Run it on any tty; it writes its answer to stdout and nothing to your terminal.
"""

from __future__ import annotations

import json
import os
import pty
import select
import threading
import time

from charter_textual_repos.ui import ReposApp


def main() -> int:
    master, slave = pty.openpty()
    os.dup2(slave, 0)
    os.dup2(slave, 1)
    os.dup2(slave, 2)

    out = {"raised_out_of_run": [], "started": False, "bytes_to_the_pane": 0}
    app = ReposApp(gathered={"repos": [], "gathered_at": time.time()}, note="thread")

    def go() -> None:
        try:
            app.run(mouse=True)                 # the non-headless driver, off-main-thread
        except BaseException as exc:            # noqa: BLE001
            out["raised_out_of_run"].append(f"{type(exc).__name__}: {exc}")

    thread = threading.Thread(target=go, daemon=True)
    thread.start()
    deadline = time.monotonic() + 4
    while time.monotonic() < deadline:
        if app.is_running:
            out["started"] = True
            break
        time.sleep(0.02)
    time.sleep(1.0)

    blob = b""
    while select.select([master], [], [], 0.1)[0]:
        blob += os.read(master, 65536)
    plain = blob.decode("utf-8", "replace")
    out["bytes_to_the_pane"] = len(blob)
    out["pane_got_a_traceback"] = "Traceback" in plain
    out["reason"] = next((ln for ln in plain.splitlines() if "ValueError" in ln), "")

    os.write(2, b"")                            # nothing; the answer goes to the file
    with open("/tmp/m10_thread_takeover.json", "w") as fh:
        json.dump(out, fh, indent=2)
    os._exit(0)


if __name__ == "__main__":
    main()
