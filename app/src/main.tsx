import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { attach } from "./bench";
import { settleLayout } from "./regions";
import { drawWindowText, listenForSizeKeys, onTextSizes, textSizes } from "./textSize";
import { DEFAULT_THEME, drawIn } from "./theme/theme";
import { theirTheme } from "./windowprefs";

// The window's colours, before anything is rendered and therefore before anything is painted:
// the operator's own `theme.json` when there is one, else the built-in compiled into the bundle.
// Neither is a read from disk on the way to the first frame — the file was read by the Rust side
// before the window existed and handed to it with the window (`windowprefs.ts`) — which is the
// only arrangement ADR 0026's 2 s cold start can afford.
drawIn(theirTheme() ?? DEFAULT_THEME);

// The window's text size, for the same reason and from the same place: the layout file the
// window was handed (`textSize.ts`, charter-app#283). The root's font size is what every `rem`
// in the stylesheet is measured by, so the whole window is drawn at it from the first frame,
// and again whenever it changes. The size keys — ⌘/Ctrl with =, - and 0 — are listened for on
// the window from here on, so they work before any project is open.
drawWindowText(textSizes().window);
onTextSizes((sizes) => drawWindowText(sizes.window));
listenForSizeKeys(window);

// In a scenario-test build only, WebdriverIO's window-side plugin, which its Tauri service
// looks for. `vite build --mode e2e` is the only build that sets this.
if (import.meta.env.VITE_E2E) void import("@wdio/tauri-plugin");

// The seam `tools/bench.mjs` measures the window through, in that same build and no other.
attach();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

// The theme an approved extension contributes, if one does, is drawn by `App` once it knows which
// project is in front — that project decides whose (ADR 0048) — and so after the render above,
// never before it: it is a disk read and a fingerprint of every file each installed extension
// declares (ADR 0041's named cost), which must not sit between the process starting and the
// first frame.

// What the layout file cost, said in the alerts drawer, and the one-time move of the arrangement
// web storage used to hold into the file. After the render for the same reason: the first frame
// was already drawn from what the window was handed.
void settleLayout();
