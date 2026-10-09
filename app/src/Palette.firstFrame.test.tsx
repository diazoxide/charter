import { afterEach, describe, expect, it } from "vitest";
import { createRoot, type Root } from "react-dom/client";
import { Palette } from "./Palette";
import type { Offer, Ran } from "./actions";

/**
 * **The palette hears `F2` from the window's first frame** (#1469).
 *
 * A key pressed the moment the window appears must open it: the operator's first `F2` of a
 * launch, and the first scenario spec's, which used to find the window drawn and its key lost.
 * React runs an effect after the browser paints, so a listener added in one leaves a gap
 * between the window being drawn and the key being heard. This renders the way `main.tsx`
 * does — `createRoot`, outside `act`, which flushes effects early and would hide the gap — and
 * presses the key as soon as the window has anything drawn in it.
 */

const OFFERS: Offer[] = [
  { id: "chat.new", title: "New tab", available: true, reason: "", does: { verb: "nothing" } },
];
const ok = (): Ran => ({ ok: true });

const scope = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean };
let root: Root | undefined;
let host: HTMLElement | undefined;
let wasAct: boolean | undefined;

afterEach(() => {
  root?.unmount();
  host?.remove();
  root = undefined;
  host = undefined;
  scope.IS_REACT_ACT_ENVIRONMENT = wasAct;
});

describe("the palette at launch", () => {
  it("opens on an F2 pressed as soon as the window is drawn", async () => {
    wasAct = scope.IS_REACT_ACT_ENVIRONMENT;
    scope.IS_REACT_ACT_ENVIRONMENT = false;
    host = document.createElement("div");
    document.body.append(host);
    const drawn = host;

    // The key, sent the moment the first frame's markup is in the document.
    const pressed = new Promise<void>((done) => {
      const seen = new MutationObserver(() => {
        seen.disconnect();
        window.dispatchEvent(new KeyboardEvent("keydown", { key: "F2", bubbles: true }));
        done();
      });
      seen.observe(drawn, { childList: true, subtree: true });
    });

    root = createRoot(drawn);
    root.render(
      <>
        <main className="window">drawn</main>
        <Palette offers={OFFERS} onRun={ok} />
      </>,
    );
    await pressed;

    await expect
      .poll(() => document.querySelector('[role="dialog"][aria-label="Command palette"]'))
      .not.toBeNull();
  });
});
