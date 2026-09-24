import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import type { PlaneChanged, PlaneId } from "./bindings";

/**
 * How many times the core has said one of `planes` changed on disk (charter-app#264).
 *
 * **A count, so it goes in a dependency array.** Every read of the plane that must follow the
 * disk — the workspace's panels, the sidebar, the alerts — lists it, and runs again exactly
 * when it moves. The core debounces (`app/src-tauri/src/planewatch.rs`), so one `git pull` is
 * one bump rather than ten.
 *
 * **Filtered on the plane, as `chat-moved` is.** The event is emitted on the app and a
 * process holds several projects, so a todo closed in one must not make another read itself
 * again.
 *
 * Listening starts at the mount, before any answer it would invalidate is asked for — the
 * reason `chatState.ts` gives for the same order. A window that cannot listen (a unit test,
 * a webview being torn down) is simply never bumped, and reads the plane when it is focused,
 * as it did before there was anything to listen to.
 */
export function usePlaneChanged(planes: readonly PlaneId[]): number {
  const [count, setCount] = useState(0);
  // The list by value: a fresh array from the caller each render must not re-register.
  const holding = planes.join("\n");

  useEffect(() => {
    const mine = new Set(holding.split("\n"));
    let gone = false;
    let stop: (() => void) | undefined;
    void (async () => {
      try {
        const unlisten = await listen<PlaneChanged>("plane-changed", (event) => {
          if (!gone && mine.has(event.payload.plane)) setCount((was) => was + 1);
        });
        if (gone) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in. See the docstring.
      }
    })();
    return () => {
      gone = true;
      stop?.();
    };
  }, [holding]);

  return count;
}
