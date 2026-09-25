import { afterEach, describe, expect, it } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";

import { useExtensionFacts } from "./extensionFacts";

/**
 * **An extension that heard an event shows what it refreshed** (charter-app#343).
 *
 * The core tells the extensions that hear an event on a thread of its own, after the command
 * that did it has answered, and then says so on `extension-heard` (`heard.rs`). That is when a
 * facts file an extension refreshed from the event has changed, and when a note about one that
 * could not be told has been kept — so the window reads the facts again, for that project only.
 */

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

function core() {
  const asked: { plane: unknown }[] = [];
  const listeners = new Map<string, number[]>();
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    if (cmd === "plugin:event|listen") {
      const { event, handler } = a as { event: string; handler: number };
      listeners.set(event, [...(listeners.get(event) ?? []), handler]);
      return handler;
    }
    if (cmd === "extension_facts") {
      asked.push({ plane: a.plane });
      return { badges: [], columns: [], notes: [`note ${asked.length}`] };
    }
    return null;
  });
  return {
    asked,
    heard: (plane: string) => {
      const handlers = listeners.get("extension-heard") ?? [];
      if (handlers.length === 0) throw new Error("the window is not listening for events heard");
      for (const handler of handlers)
        window.__TAURI_INTERNALS__.runCallback(handler, {
          event: "extension-heard",
          id: 1,
          payload: { plane },
        });
    },
  };
}

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("the facts an extension shows", () => {
  it("are read again once the extensions that hear an event in this project were told", async () => {
    const { asked, heard } = core();
    const { result } = renderHook(() => useExtensionFacts("/plane", "alpha"));
    await waitFor(() => expect(result.current.notes).toEqual(["note 1"]));

    await waitFor(() => heard("/plane"));

    await waitFor(() => expect(result.current.notes).toEqual(["note 2"]));
    expect(asked).toHaveLength(2);
  });

  it("are not read again for an event in another project", async () => {
    const { asked, heard } = core();
    const { result } = renderHook(() => useExtensionFacts("/plane", "alpha"));
    await waitFor(() => expect(result.current.notes).toEqual(["note 1"]));

    await waitFor(() => heard("/elsewhere"));
    // Give a read that should not happen the chance to.
    await new Promise((settle) => setTimeout(settle, 50));

    expect(asked).toHaveLength(1);
  });
});
