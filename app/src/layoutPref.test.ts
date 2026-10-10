import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { layoutPref } from "./layoutPref";
import { aboutThisMachine, GLOBAL, sayAboutThisMachine } from "./windowprefs";

const PATH = "/home/op/.config/purlis/layout.json";

const handed = (document: unknown, trouble: string | null = null) => {
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: PATH, found: true, document, trouble },
    theme: { path: "", found: false, document: null, trouble: null },
  };
};

/** A preference of one number, `size`, which is 3 unless the file says another. */
const sized = () =>
  layoutPref<number>({
    key: "size",
    fallback: 3,
    load: (raw) => {
      const held = (raw as { size?: unknown }).size;
      if (held === undefined) return { value: 3, said: [] };
      if (typeof held === "number") return { value: held, said: [] };
      return { value: 3, said: [`"size" ${JSON.stringify(held)} is not a number, so it is 3`] };
    },
    same: (one, other) => one === other,
    written: (value) => (value === 3 ? undefined : value),
    remedy: (where) => `fix ${where}, or change the size in Settings, which rewrites it`,
  });

beforeEach(() => sayAboutThisMachine("size", undefined));
afterEach(() => Reflect.deleteProperty(globalThis, GLOBAL));

describe("a preference kept in the layout file (#1686)", () => {
  it("is what the file holds, read once, and the default where the file says nothing", () => {
    handed({ size: 5 });
    const size = sized();
    expect(size.value()).toBe(5);
    // Read once: the file the window was handed is not read again this launch.
    handed({ size: 9 });
    expect(size.value()).toBe(5);

    handed({});
    expect(sized().value()).toBe(3);
  });

  it("is the default for a file purlis refused, which the layout says once, not this", () => {
    handed({ size: 5 }, "it is version 9");
    expect(sized().value()).toBe(3);
    expect(aboutThisMachine().some((row) => row.subject === "size")).toBe(false);
  });

  it("says what it put right, with the file's path and the way to fix it, until it changes", () => {
    handed({ size: "big" });
    const size = sized();
    expect(size.value()).toBe(3);
    const row = aboutThisMachine().find((one) => one.subject === "size");
    expect(row).toMatchObject({
      severity: "warn",
      detail: `${PATH}: "size" "big" is not a number, so it is 3`,
      remedy: `fix ${PATH}, or change the size in Settings, which rewrites it`,
    });

    size.set(4);
    expect(aboutThisMachine().some((one) => one.subject === "size")).toBe(false);
  });

  it("tells its listeners of a change, and of nothing that changed nothing", () => {
    handed({});
    const size = sized();
    const heard: number[] = [];
    const stop = size.on((value) => heard.push(value));

    size.set(3);
    size.set(4);
    size.set(4);
    stop();
    size.set(5);

    expect(heard).toEqual([4]);
    expect(size.value()).toBe(5);
  });

  it("keeps a value without telling its listeners, and still redraws what draws it", () => {
    handed({});
    const size = sized();
    const heard: number[] = [];
    size.on((value) => heard.push(value));
    const { result } = renderHook(() => size.use());

    act(() => size.keep(7));

    expect(result.current).toBe(7);
    expect(heard).toEqual([]);
  });

  it("is written only where it is not the default", () => {
    handed({});
    const size = sized();
    expect(size.written()).toBeUndefined();
    size.set(6);
    expect(size.written()).toBe(6);
  });

  it("forgets what this launch chose and read, as a new launch would", () => {
    handed({ size: 5 });
    const size = sized();
    size.set(8);
    size.forget();
    handed({ size: 2 });
    expect(size.value()).toBe(2);
  });
});
