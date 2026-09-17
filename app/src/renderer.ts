import type { Terminal } from "@xterm/xterm";

/**
 * Which renderer a pane's terminal draws with.
 *
 * `dom` is xterm.js's own default: rows as DOM elements, no GPU context. `webgl` is
 * `@xterm/addon-webgl`, which draws into a WebGL context of its own — one per terminal, and
 * WebKit gives a page only so many (research §5.1). Which one the app uses is measured, not
 * assumed: `tools/bench.mjs` runs both arms against the spec's limits.
 */
export type Renderer = "dom" | "webgl";

/** The renderer the app draws with. The benchmark is what changes this. */
const SHIPPED: Renderer = "dom";

let chosen: Renderer = SHIPPED;

/** What panes opened from now on will draw with. */
export function renderer(): Renderer {
  return chosen;
}

/** Draws panes opened from now on with `kind`. Panes already on screen keep theirs. */
export function drawWith(kind: Renderer): void {
  chosen = kind;
}

/** What a pane knows about its own renderer. It changes if a WebGL context is lost later. */
export type Drawing = {
  renderer: Renderer;
  /** Whether the renderer asked for is the one drawing now. */
  active: boolean;
  /** Why it is not, when it is not: a WebGL context refused or lost. */
  trouble?: string;
};

/**
 * Loads the chosen renderer into `pane`, and answers with what is drawing.
 *
 * A WebGL context that cannot be created, or that is lost later, leaves the pane on the DOM
 * renderer rather than blank. WebKit takes the oldest context back when a page asks for one
 * too many (research §5.1), so this is what happens to a pane once enough others are open.
 */
export async function draw(pane: Terminal, onTrouble?: (why: string) => void): Promise<Drawing> {
  const kind = chosen;
  if (kind === "dom") return { renderer: "dom", active: true };
  const drawing: Drawing = { renderer: "webgl", active: false };
  const fallBack = (why: string) => {
    drawing.active = false;
    drawing.trouble = why;
    onTrouble?.(`drawing without WebGL: ${why}`);
  };
  try {
    const { WebglAddon } = await import("@xterm/addon-webgl");
    const webgl = new WebglAddon();
    webgl.onContextLoss(() => {
      webgl.dispose();
      fallBack("the WebGL context was lost");
    });
    pane.loadAddon(webgl);
    drawing.active = true;
  } catch (err: unknown) {
    fallBack(String(err));
  }
  return drawing;
}
