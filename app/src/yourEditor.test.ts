import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, renderHook } from "@testing-library/react";
import {
  CHOOSE_EDITOR,
  EDITORS,
  forgetYourEditor,
  loadEditor,
  setYourEditor,
  useYourEditor,
  yourEditor,
} from "./yourEditor";
import { aboutThisMachine, GLOBAL, sayAboutThisMachine } from "./windowprefs";
import { youGroups } from "./settings/you";

const PATH = "/home/op/.config/charter/layout.json";

const handed = (document: unknown) => {
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: PATH, found: true, document, trouble: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
};

beforeEach(() => {
  forgetYourEditor();
  sayAboutThisMachine("editor", undefined);
});
afterEach(() => Reflect.deleteProperty(globalThis, GLOBAL));

describe("which editor is yours (RC-20)", () => {
  it("is none until the operator chooses one: charter does not guess", () => {
    expect(loadEditor({ version: 1, regions: [] })).toEqual({ editor: undefined, said: [] });
  });

  it("is read from the layout file", () => {
    for (const editor of ["vscode", "zed", "idea", "variable"] as const) {
      expect(loadEditor({ editor })).toEqual({ editor, said: [] });
    }
  });

  it("is none when the file names one purlis does not know, and says so", () => {
    const { editor, said } = loadEditor({ editor: "emacs; rm -rf /" });

    expect(editor).toBeUndefined();
    expect(said.join()).toContain('"emacs; rm -rf /"');
  });

  it("says what the file got wrong in the alerts drawer, once read", () => {
    handed({ version: 1, regions: [], editor: 7 });

    expect(yourEditor()).toBeUndefined();
    expect(JSON.stringify(aboutThisMachine())).toContain(PATH);
    // Linked to where it is fixed (SE-22).
    expect(aboutThisMachine()[0].settings).toBe("you.editor");
  });

  it("is the one the launch started from, until it is changed", () => {
    handed({ version: 1, regions: [], editor: "zed" });
    const { result } = renderHook(() => useYourEditor());
    expect(result.current).toBe("zed");

    act(() => setYourEditor("vscode"));

    expect(result.current).toBe("vscode");
    expect(yourEditor()).toBe("vscode");
  });

  it("names each editor for a person", () => {
    expect(EDITORS.map((one) => one.name)).toEqual([
      "VS Code",
      "Zed",
      "A JetBrains IDE",
      "$VISUAL or $EDITOR",
    ]);
  });
});

describe("the way to choose your editor (#1201, #1244)", () => {
  it("names the Editor group and the setting it holds, so the link lands on the choice", () => {
    const group = youGroups().find((one) => one.id === CHOOSE_EDITOR.group);

    expect(group?.label).toBe("Editor");
    expect(group?.settings.map((one) => one.id)).toContain(CHOOSE_EDITOR.setting);
  });
});
