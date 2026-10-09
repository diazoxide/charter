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
      prefs: { lines: 2, grouped: false, tabbed: false, away: true },
      said: [],
    });
    expect(chatsListPrefs()).toEqual(DEFAULT_CHATS_LIST);
  });

  it("is read from the layout file, each field on its own", () => {
    expect(loadChatsList({ chats: { lines: 1, grouped: true, tabbed: false } }).prefs).toEqual({
      lines: 1,
      grouped: true,
      tabbed: false,
      away: true,
    });
    expect(loadChatsList({ chats: { grouped: true, tabbed: false } }).prefs).toEqual({
      lines: 2,
      grouped: true,
      tabbed: false,
      away: true,
    });
    expect(loadChatsList({ chats: { lines: 1 } }).prefs).toEqual({
      lines: 1,
      grouped: false,
      tabbed: false,
      away: true,
    });
  });

  it("opens a pressed task inside its session's tab until the person says otherwise (#1489)", () => {
    expect(DEFAULT_CHATS_LIST.tabbed).toBe(false);
    expect(loadChatsList({ chats: { tabbed: true } }).prefs).toEqual({
      lines: 2,
      grouped: false,
      tabbed: true,
      away: true,
    });
    const { prefs, said } = loadChatsList({ chats: { tabbed: "always" } });
    expect(prefs.tabbed).toBe(false);
    expect(said.join()).toContain('"chats.tabbed" "always"');
  });

  it("sums up what happened while the person was away until they turn it off (#1514)", () => {
    expect(DEFAULT_CHATS_LIST.away).toBe(true);
    expect(loadChatsList({ chats: { away: false } }).prefs).toEqual({
      lines: 2,
      grouped: false,
      tabbed: false,
      away: false,
    });
    const { prefs, said } = loadChatsList({ chats: { away: "never" } });
    expect(prefs.away).toBe(true);
    expect(said.join()).toContain('"chats.away" "never" is not true or false, so it is true');
  });

  it("writes that choice to the layout file with the others", async () => {
    const written: string[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "write_layout") written.push((args as { text: string }).text);
      return null;
    });

    setChatsListPrefs({ tabbed: true });
    await expect.poll(() => written.length).toBe(1);
    expect(JSON.parse(written[0]).chats).toEqual({
      lines: 2,
      grouped: false,
      tabbed: true,
      away: true,
    });
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
    expect(result.current).toEqual({ lines: 1, grouped: false, tabbed: false, away: true });

    act(() => setChatsListPrefs({ grouped: true, tabbed: false }));

    expect(result.current).toEqual({ lines: 1, grouped: true, tabbed: false, away: true });
  });

  it("is written to the layout file as it is changed, and left out of it at the default", async () => {
    const written: string[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "write_layout") written.push((args as { text: string }).text);
      return null;
    });

    setChatsListPrefs({ lines: 1 });
    await expect.poll(() => written.length).toBe(1);
    expect(JSON.parse(written[0]).chats).toEqual({
      lines: 1,
      grouped: false,
      tabbed: false,
      away: true,
    });

    setChatsListPrefs({ lines: 2 });
    await expect.poll(() => written.length).toBe(2);
    expect("chats" in JSON.parse(written[1])).toBe(false);
  });
});
