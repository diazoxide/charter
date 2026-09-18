/**
 * What `withGlobalTauri` puts on the window, as much of it as the scenario tests use.
 *
 * Only a build made with `tauri.e2e.conf.json` has it, which is why this lives beside the
 * scenario tests and not in the app's own types: nothing the operator is given carries it.
 */
declare global {
  interface Window {
    __TAURI__: {
      core: { invoke: (command: string, args?: unknown) => Promise<unknown> };
      window: { getCurrentWindow: () => { close: () => Promise<void>; show: () => Promise<void> } };
    };
  }
}

export {};
