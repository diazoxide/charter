import { describe, expect, it } from "vitest";
import type { OpenChat } from "./bindings";
import {
  AT_MOST,
  AT_MOST_PER_CHAT,
  FADE_MS,
  faded,
  nextFade,
  touched,
  touchingIn,
  touchSaid,
  type Touches,
} from "./touching";

const chat = (session: number, name: string, cwd: string | null) =>
  ({ session, name, cwd }) as unknown as OpenChat;

describe("the touching map", () => {
  it("keeps a touched file until its chat has been quiet on it, then lets it fade", () => {
    let map: Touches = touched([], 1, "src/a.rs", 0);
    expect(faded(map, FADE_MS - 1)).toBe(map);
    // Touched again: the quiet starts over.
    map = touched(map, 1, "src/a.rs", 3000);
    expect(map).toEqual([{ session: 1, path: "src/a.rs", at: 3000 }]);
    expect(faded(map, 3000 + FADE_MS - 1)).toHaveLength(1);
    expect(faded(map, 3000 + FADE_MS)).toEqual([]);
  });

  it("lets a file the chat moved on from fade while the one it is on stays", () => {
    let map: Touches = touched([], 1, "a.rs", 0);
    map = touched(map, 1, "b.rs", 2000);
    expect(faded(map, FADE_MS).map((one) => one.path)).toEqual(["b.rs"]);
    expect(nextFade(map)).toBe(FADE_MS);
    expect(nextFade([])).toBeUndefined();
  });

  it("holds a bounded number of paths per chat and in all", () => {
    let map: Touches = [];
    for (let n = 0; n < 50; n++) map = touched(map, 1, `f${n}`, n);
    expect(map).toHaveLength(AT_MOST_PER_CHAT);
    expect(map[map.length - 1].path).toBe("f49");
    for (let chat = 0; chat < 100; chat++)
      for (let n = 0; n < AT_MOST_PER_CHAT; n++) map = touched(map, chat + 2, `f${n}`, 100);
    expect(map.length).toBe(AT_MOST);
  });
});

describe("which files of a branch are being touched", () => {
  const chats = [
    chat(1, "fix login", "/w/ws/.pieces/app/fix"),
    chat(2, "docs", "/w/ws/.pieces/app/fix/docs"),
    chat(3, "other branch", "/w/ws/.pieces/app/fix-two"),
    chat(4, "nowhere", null),
  ];

  it("marks the file, every folder above it and the branch, naming the chats", () => {
    const touches = touched(touched([], 1, "src/auth/login.ts", 0), 2, "guide.md", 0);
    const marks = touchingIn(touches, chats, "/w/ws/.pieces/app/fix");
    expect(Object.fromEntries(marks)).toEqual({
      "": ["fix login", "docs"],
      src: ["fix login"],
      "src/auth": ["fix login"],
      "src/auth/login.ts": ["fix login"],
      docs: ["docs"],
      "docs/guide.md": ["docs"],
    });
  });

  it("marks nothing of a branch the chat does not work in", () => {
    const touches = touched([], 3, "src/a.ts", 0);
    expect(touchingIn(touches, chats, "/w/ws/.pieces/app/fix").size).toBe(0);
    expect(touchingIn(touched([], 4, "a.ts", 0), chats, "/w/ws/.pieces/app/fix").size).toBe(0);
    expect(touchingIn(touches, chats, undefined).size).toBe(0);
  });

  it("reads a folder written with backslashes", () => {
    const windows = [chat(1, "win", "C:\\w\\fix\\")];
    const marks = touchingIn(touched([], 1, "a.ts", 0), windows, "C:\\w\\fix");
    expect(marks.get("a.ts")).toEqual(["win"]);
  });

  it("says who is working on a file", () => {
    expect(touchSaid(["fix login"])).toBe("fix login is working here now");
    expect(touchSaid(["a", "b", "c"])).toBe("a, b and c are working here now");
  });
});
