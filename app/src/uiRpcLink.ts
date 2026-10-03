// The UI RPC's transport (FD-26, ADR 0068 §4): what `uiRpc.ts`, the typed client generated
// from `ipc_commands.rs`, calls instead of Tauri's `invoke`.
//
// Each command goes on the control lane of the link to `charterd`, beside the session protocol,
// as `{"ui":{"call":{"id":…,"method":…,"args":…}}}`, and its answer comes back as
// `{"ui":{"reply":{"re":…,"ok":…}}}` or `…"err":…`. The host serves it only to the app's window
// (`local-ui`) and only to its own build, so the window says which build it is first
// (`openUiRpc`). `charter_session_protocol::ui` is the host's side of the same frames.
//
// What carries the frames is the caller's: a link to `charterd` once FD-5 starts one.

export { Channel } from "@tauri-apps/api/core";
import { Channel } from "@tauri-apps/api/core";

/** One frame of the control lane each way, as JSON text. */
export interface UiRpcTransport {
  send(frame: string): void;
  /** Called with every frame the host sends; the UI RPC reads its own and passes over the rest. */
  listen(heard: (frame: string) => void): void;
}

type Waiting = { resolve: (value: unknown) => void; reject: (why: unknown) => void };

type UiFrame =
  | { welcome: { build: string } }
  | { refused: { why: string } }
  | { reply: { re: number; ok?: unknown; err?: unknown } };

let link: {
  transport: UiRpcTransport;
  nextId: number;
  calls: Map<number, Waiting>;
  hello: Waiting | null;
  open: boolean;
} | null = null;

/**
 * Open the UI RPC on `transport` as this build of the app, closing any link opened before.
 * Resolves once the host welcomes it, and rejects with the host's sentence when it refuses:
 * another build, or another scope.
 */
export function openUiRpc(transport: UiRpcTransport, build: string): Promise<void> {
  // One link at a time: what still waits on the old one is told it ended, and its late
  // answers reach nobody.
  closeUiRpc();
  const opening = {
    transport,
    nextId: 1,
    calls: new Map<number, Waiting>(),
    hello: null as Waiting | null,
    open: false,
  };
  link = opening;
  transport.listen((text) => heard(opening, text));
  return new Promise<void>((resolve, reject) => {
    opening.hello = { resolve: () => resolve(), reject };
    transport.send(JSON.stringify({ ui: { hello: { build } } }));
  });
}

/** Forget the link: every call waiting is told it ended. */
export function closeUiRpc(): void {
  if (link === null) return;
  for (const waiting of link.calls.values()) waiting.reject(new Error("the UI RPC's link ended"));
  link.calls.clear();
  link.hello?.reject(new Error("the UI RPC's link ended"));
  link.hello = null;
  link.open = false;
  link = null;
}

/**
 * Tauri's `invoke`, over the link: `method` is the command's name and `args` its arguments by
 * name. A command's own error rejects with that error, as Tauri's does, so the generated client's
 * typed results read it the same way. Anything else that goes wrong rejects with an `Error`.
 */
export function invoke<T>(method: string, args?: Record<string, unknown>): Promise<T> {
  const open = link;
  if (open === null || !open.open) {
    return Promise.reject(new Error("the UI RPC is not open: openUiRpc first"));
  }
  if (args !== undefined && Object.values(args).some((arg) => arg instanceof Channel)) {
    return Promise.reject(
      new Error(`\`${method}\` takes a channel, which only a view of the session protocol carries`),
    );
  }
  const id = open.nextId++;
  return new Promise<T>((resolve, reject) => {
    open.calls.set(id, { resolve: (value) => resolve(value as T), reject });
    open.transport.send(JSON.stringify({ ui: { call: { id, method, args: args ?? {} } } }));
  });
}

function heard(on: NonNullable<typeof link>, text: string): void {
  let frame: unknown;
  try {
    frame = JSON.parse(text);
  } catch {
    return;
  }
  if (typeof frame !== "object" || frame === null || !("ui" in frame)) return;
  const ui = frame.ui as UiFrame;
  if ("welcome" in ui) {
    on.open = true;
    on.hello?.resolve(undefined);
    on.hello = null;
  } else if ("refused" in ui) {
    on.hello?.reject(new Error(ui.refused.why));
    on.hello = null;
  } else if ("reply" in ui) {
    const waiting = on.calls.get(ui.reply.re);
    if (waiting === undefined) return;
    on.calls.delete(ui.reply.re);
    if ("err" in ui.reply) waiting.reject(ui.reply.err);
    else waiting.resolve(ui.reply.ok);
  }
}
