import { describe, expect, it } from "vitest";
import type { ChatDoing, Doing } from "./bindings";
import { doingSays, saidWhole, told, type Doings } from "./chatDoing";

const NOTHING: Doings = { bySession: {}, heardAt: {} };

const doing = (kind: string, name: string | null = null, count = 0): Doing => ({
  kind,
  name,
  count,
});

const telling = (session: number, sequence: number, what: Doing | null): ChatDoing => ({
  plane: "/plane",
  session,
  sequence,
  doing: what,
});

const said = (what: Doing) => {
  const says = doingSays(what);
  return says === undefined ? undefined : saidWhole(says);
};

describe("what a working chat's line says (#1493)", () => {
  it("says each kind in purlis's own words", () => {
    expect(said(doing("thinking"))).toBe("thinking");
    expect(said(doing("command"))).toBe("running a command");
    expect(said(doing("command", "cargo"))).toBe("running cargo");
    expect(said(doing("editing"))).toBe("editing a file");
    expect(said(doing("editing", "Notice.tsx"))).toBe("editing Notice.tsx");
    expect(said(doing("reading", null, 1))).toBe("reading a file");
    expect(said(doing("reading", "state.rs", 1))).toBe("reading state.rs");
    expect(said(doing("reading", null, 3))).toBe("reading 3 files");
    expect(said(doing("searching"))).toBe("searching");
    expect(said(doing("fetching"))).toBe("fetching a page");
    expect(said(doing("helper"))).toBe("waiting on a helper");
    expect(said(doing("dispatching"))).toBe("dispatching a task");
    expect(said(doing("asking"))).toBe("asking a question");
    expect(said(doing("reporting"))).toBe("writing its report");
    expect(said(doing("tool"))).toBe("using a tool");
  });

  it("keeps the name apart from the words, for the kinds that have one", () => {
    expect(doingSays(doing("editing", "a.rs"))).toEqual({ words: "editing", name: "a.rs" });
    expect(doingSays(doing("thinking"))).toEqual({ words: "thinking" });
  });

  it("says no name for a kind that has none, whatever it was sent", () => {
    for (const kind of [
      "thinking",
      "searching",
      "fetching",
      "helper",
      "dispatching",
      "asking",
      "reporting",
      "tool",
    ])
      expect(doingSays(doing(kind, "needs you"))?.name, kind).toBeUndefined();
    // Past the first read no one file is named.
    expect(doingSays(doing("reading", "a.rs", 2))).toEqual({ words: "reading 2 files" });
    expect(said(doing("command", ""))).toBe("running a command");
  });

  it("says nothing at all for a kind it has no sentence for", () => {
    for (const kind of ["", "notice", "needs you", "Allow once", "constructor", "__proto__"])
      expect(doingSays(doing(kind, "x")), kind).toBeUndefined();
  });
});

describe("what is held of the chats' lines (#1493)", () => {
  it("replaces a chat's line with the next one, and takes it away at the turn's end", () => {
    let held = told(NOTHING, telling(4, 1, doing("thinking")));
    expect(held.bySession[4]).toEqual(doing("thinking"));
    held = told(held, telling(4, 2, doing("command", "cargo")));
    expect(held.bySession[4]).toEqual(doing("command", "cargo"));
    held = told(held, telling(4, 3, null));
    expect(held.bySession).toEqual({});
  });

  it("does not take a telling older than the one it holds of that chat", () => {
    let held = told(NOTHING, telling(4, 5, null));
    held = told(held, telling(4, 3, doing("command", "cargo")));
    expect(held.bySession[4]).toBeUndefined();
    // Another chat's is its own, whatever its number.
    held = told(held, telling(5, 2, doing("thinking")));
    expect(held.bySession[5]).toEqual(doing("thinking"));
  });

  it("keeps the lines it held when a telling says the same thing again", () => {
    const held = told(NOTHING, telling(4, 1, doing("command", "cargo")));
    const again = told(held, telling(4, 2, doing("command", "cargo")));
    expect(again.bySession).toBe(held.bySession);
    // One chat's line changing leaves another's the very object it was.
    const other = told(again, telling(5, 3, doing("thinking")));
    expect(other.bySession[4]).toBe(held.bySession[4]);
  });
});
