import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  chatsListPrefs,
  DEFAULT_CHATS_LIST,
  loadChatsList,
  setChatsListPrefs,
  useChatsListPrefs,
} from "./chatsListPrefs";
import { forgetThisLaunch } from "./regions";
import { aboutThisMachine, GLOBAL, sayAboutThisMachine } from "./windowprefs";

const PATH = "/home/op/.config/purlis/layout.json";

const handed = (document: unknown) => {
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: PATH, found: true, document, trouble: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
};

beforeEach(() => {
  forgetThisLaunch();
  sayAboutThisMachine("chats", undefined);
});
afterEach(() => {
  Reflect.deleteProperty(globalThis, GLOBAL);
  clearMocks();
});

describe("how the Chats list is drawn (#1499, V100-73)", () => {
  it("is two lines and one order until the person says otherwise", () => {
    expect(loadChatsList({ version: 1, regions: [] })).toEqual({
      prefs: { lines: 2, grouped: false },
      said: [],
    });
    expect(chatsListPrefs()).toEqual(DEFAULT_CHATS_LIST);
  });

  it("is read from the layout file, each field on its own", () => {
    expect(loadChatsList({ chats: { lines: 1, grouped: true } }).prefs).toEqual({
      lines: 1,
      grouped: true,
    });
    expect(loadChatsList({ chats: { grouped: true } }).prefs).toEqual({ lines: 2, grouped: true });
    expect(loadChatsList({ chats: { lines: 1 } }).prefs).toEqual({ lines: 1, grouped: false });
  });

  it("is the default where the file says something else, and says so", () => {
    const { prefs, said } = loadChatsList({ chats: { lines: 3, grouped: "yes" } });

    expect(prefs).toEqual(DEFAULT_CHATS_LIST);
    expect(said.join()).toContain('"chats.lines" 3');
    expect(said.join()).toContain('"chats.grouped" "yes"');
    expect(loadChatsList({ chats: [1] }).said).toHaveLength(1);
  });

  it("says what the file got wrong in the alerts drawer, linked to where it is fixed", () => {
    handed({ version: 1, regions: [], chats: { lines: "two" } });

    expect(chatsListPrefs()).toEqual(DEFAULT_CHATS_LIST);
    expect(JSON.stringify(aboutThisMachine())).toContain(PATH);
    expect(aboutThisMachine()[0].settings).toBe("you.chats");
  });

  it("is what the launch started from, until it is changed", () => {
    handed({ version: 1, regions: [], chats: { lines: 1 } });
    const { result } = renderHook(() => useChatsListPrefs());
    expect(result.current).toEqual({ lines: 1, grouped: false });

    act(() => setChatsListPrefs({ grouped: true }));

    expect(result.current).toEqual({ lines: 1, grouped: true });
  });

  it("is written to the layout file as it is changed, and left out of it at the default", async () => {
    const written: string[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "write_layout") written.push((args as { text: string }).text);
      return null;
    });

    setChatsListPrefs({ lines: 1 });
    await expect.poll(() => written.length).toBe(1);
    expect(JSON.parse(written[0]).chats).toEqual({ lines: 1, grouped: false });

    setChatsListPrefs({ lines: 2 });
    await expect.poll(() => written.length).toBe(2);
    expect("chats" in JSON.parse(written[1])).toBe(false);
  });
});
