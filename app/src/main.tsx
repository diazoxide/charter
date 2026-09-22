import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { attach } from "./bench";
import { DEFAULT_THEME, drawIn } from "./theme/theme";

// The window's colours, before anything is rendered and therefore before anything is painted.
// A built-in theme is compiled into the bundle, so this is an object already in memory and
// `TOKENS.length` calls to `setProperty` — nothing is read from disk on the way to the first
// frame, which is the only arrangement ADR 0026's 2 s cold start can afford.
drawIn(DEFAULT_THEME);

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
