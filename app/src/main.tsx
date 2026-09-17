import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

// In a scenario-test build only, WebdriverIO's window-side plugin, which its Tauri service
// looks for. `vite build --mode e2e` is the only build that sets this.
if (import.meta.env.VITE_E2E) void import("@wdio/tauri-plugin");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
