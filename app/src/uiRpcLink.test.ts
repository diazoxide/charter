// The UI RPC's typed client (`uiRpc.ts`, generated from `ipc_commands.rs`) sends each command
// on the link's control lane as the host reads it. The frames here are the ones
// `crates/session-protocol/tests/the_ui_rpc_rides_beside_the_session_protocol_for_one_build.rs`
// holds the host to, so the two ends agree on the wire.

import { afterEach, describe, expect, it } from "vitest";
import { Channel } from "@tauri-apps/api/core";
import { commands } from "./uiRpc";
import { closeUiRpc, invoke, openUiRpc, type UiRpcTransport } from "./uiRpcLink";

/** A link the test plays the host of. */
function fakeLink() {
  const sent: unknown[] = [];
  let toWindow: (frame: string) => void = () => {};
  const transport: UiRpcTransport = {
    send: (frame) => sent.push(JSON.parse(frame)),
    listen: (heard) => {
      toWindow = heard;
    },
  };
  return { transport, sent, host: (frame: string) => toWindow(frame) };
}

async function opened() {
  const link = fakeLink();
  const opening = openUiRpc(link.transport, "0.4.0+9055f7a");
  expect(link.sent).toEqual([{ ui: { hello: { build: "0.4.0+9055f7a" } } }]);
  link.host('{"ui":{"welcome":{"build":"0.4.0+9055f7a"}}}');
  await opening;
  link.sent.length = 0;
  return link;
}

afterEach(() => closeUiRpc());

describe("the UI RPC's client", () => {
  it("sends a generated command by its name with its arguments by name, and reads its value", async () => {
    const link = await opened();
    const renamed = commands.renameChat("p1", 5, "login");
    expect(link.sent).toEqual([
      {
        ui: {
          call: { id: 1, method: "rename_chat", args: { plane: "p1", session: 5, label: "login" } },
        },
      },
    ]);
    link.host('{"ui":{"reply":{"re":1,"ok":null}}}');
    expect(await renamed).toEqual({ status: "ok", data: null });
  });

  it("reads a command's own error as the typed client's error", async () => {
    const link = await opened();
    const closed = commands.closePlane("p2");
    link.host('{"ui":{"reply":{"re":1,"err":"that plane is not open"}}}');
    expect(await closed).toEqual({ status: "error", error: "that plane is not open" });
  });

  it("matches each answer to its own call, in whatever order they come", async () => {
    const link = await opened();
    const first = invoke<number>("first", {});
    const second = invoke<number>("second", {});
    link.host('{"ui":{"reply":{"re":2,"ok":2}}}');
    link.host('{"pushed":{"event":{"seq":1}}}');
    link.host('{"ui":{"reply":{"re":1,"ok":1}}}');
    expect([await first, await second]).toEqual([1, 2]);
  });

  it("is refused by a host of another build, with the host's sentence", async () => {
    const link = fakeLink();
    const opening = openUiRpc(link.transport, "0.3.9+1111111");
    link.host('{"ui":{"refused":{"why":"the UI RPC is private to one build"}}}');
    await expect(opening).rejects.toThrow("private to one build");
    await expect(invoke("rename_chat", {})).rejects.toThrow("not open");
  });

  it("sends nothing before the host welcomes it", async () => {
    const link = fakeLink();
    // Never welcomed: it ends unanswered when the link is forgotten.
    openUiRpc(link.transport, "0.4.0+9055f7a").catch(() => {});
    await expect(commands.closePlane("p2")).rejects.toThrow("not open");
    expect(link.sent).toEqual([{ ui: { hello: { build: "0.4.0+9055f7a" } } }]);
  });

  it("opening it again ends the calls still waiting on the link before", async () => {
    const before = await opened();
    const waiting = invoke("rename_chat", {});
    const after = fakeLink();
    const reopening = openUiRpc(after.transport, "0.4.0+9055f7a");
    await expect(waiting).rejects.toThrow("ended");
    // The old link's late answer reaches nobody; the new one is the one in use.
    before.host('{"ui":{"reply":{"re":1,"ok":"late"}}}');
    after.host('{"ui":{"welcome":{"build":"0.4.0+9055f7a"}}}');
    await reopening;
    void invoke("rename_chat", {}).catch(() => {});
    expect(after.sent.at(-1)).toEqual({
      ui: { call: { id: 1, method: "rename_chat", args: {} } },
    });
  });

  it("refuses a command that takes a channel, which only a view carries", async () => {
    const link = await opened();
    // A channel as the window holds one, without the Tauri runtime a test has none of.
    const output = Object.create(Channel.prototype) as Channel<string>;
    await expect(invoke("watch_session", { output })).rejects.toThrow("channel");
    expect(link.sent).toEqual([]);
  });
});
