import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { attach } from "./bench";
import { settleLayout } from "./regions";
import { DEFAULT_THEME, drawIn } from "./theme/theme";
import { theirThemeOnce } from "./windowprefs";

// The window's colours, before anything is rendered and therefore before anything is painted:
// the operator's own `theme.json` when there is one, else the built-in compiled into the bundle.
// Neither is a read from disk on the way to the first frame — the file was read by the Rust side
// before the window existed and handed to it with the window (`windowprefs.ts`) — which is the
// only arrangement ADR 0026's 2 s cold start can afford.
drawIn(theirThemeOnce() ?? DEFAULT_THEME);

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
