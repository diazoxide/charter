import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Palette, fileKeyOf } from "./Palette";
import type { Offer, Ran } from "./actions";
import type { FilesFound, FoundFile, PlaneId } from "./bindings";
import { scopeLadder } from "./fileFind";
import { forgetProjectThemes } from "./projectTheme";

/**
 * **Copy path and Reveal on ⌘P's files** (#1143): the aimed file row's path copied or revealed
 * by a key, through the same two commands the explorer's row menu runs, with a refusal said on
 * the palette's own line. Off a Mac (jsdom's platform is empty) the keys are Shift+Alt+C and
 * Shift+Alt+R; on a Mac, ⌥⌘C and ⌥⌘R.
 */

afterEach(() => {
  cleanup();
  clearMocks();
  forgetProjectThemes();
  vi.restoreAllMocks();
});

const PLANE = "/projects/alpha" as unknown as PlaneId;
const BRANCH = { workspace: "web", repo: "svc", piece: "fix-login" };

const lone: Offer = {
  id: `project.select:${PLANE}`,
  title: "Switch to project alpha",
  name: "alpha",
  available: false,
  reason: "It is already in front.",
  does: { verb: "selectProject", plane: PLANE as unknown as string },
};

const file = (path: string): FoundFile => ({
  plane: PLANE,
  ...BRANCH,
  path,
  matched: [],
});

/** The core: `find_files` finds `found`; the two path commands answer `path`, and every ask
 *  is kept. */
function core(found: FoundFile[], path: (cmd: string) => unknown = () => null) {
  const asked: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "find_files") {
      const answer: FilesFound = { files: found, branches: 1, refused: [], partial: [] };
      return answer;
    }
    if (cmd === "copy_branch_path" || cmd === "reveal_branch_path") {
      asked.push({ cmd, args });
      return path(cmd);
    }
    return null;
  });
  return asked;
}

const ok = (): Ran => ({ ok: true });

function opened() {
  const files = {
    ladder: scopeLadder(PLANE, BRANCH, 1),
    nameOf: () => "alpha",
    onOpen: vi.fn(),
  };
  const view = render(
    <Palette offers={[]} projects={{ rows: [lone], asked: 0 }} files={files} onRun={ok} />,
  );
  view.rerender(
    <Palette offers={[]} projects={{ rows: [lone], asked: 1 }} files={files} onRun={ok} />,
  );
  return files.onOpen;
}

describe("⌘P's file rows", () => {
  it("copy the aimed file's path through the explorer's command, and say so", async () => {
    const asked = core([file("src/login.ts")]);
    const onOpen = opened();
    await userEvent.keyboard("login");
    await screen.findByRole("listbox", { name: "Files" });

    await userEvent.keyboard("{Shift>}{Alt>}c{/Alt}{/Shift}");

    expect(await screen.findByText("Copied the path of src/login.ts.")).toBeTruthy();
    expect(asked).toEqual([
      {
        cmd: "copy_branch_path",
        args: { plane: PLANE, ...BRANCH, path: "src/login.ts", absolute: false },
      },
    ]);
    // The palette stays, so the person can go on finding; nothing was opened.
    expect(screen.getByRole("listbox", { name: "Files" })).toBeTruthy();
    expect(onOpen).not.toHaveBeenCalled();
  });

  it("reveal the aimed file, and say a refusal in the core's words", async () => {
    const asked = core([file("src/login.ts")], () => {
      throw "src/login.ts is not in the branch any more";
    });
    opened();
    await userEvent.keyboard("login");
    await screen.findByRole("listbox", { name: "Files" });

    await userEvent.keyboard("{Shift>}{Alt>}r{/Alt}{/Shift}");

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "src/login.ts is not in the branch any more",
    );
    expect(asked.map((one) => one.cmd)).toEqual(["reveal_branch_path"]);
    // A new query is a new question: the old answer goes.
    await userEvent.keyboard("x");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("say their keys while a file is aimed, and only then", async () => {
    core([file("src/login.ts")]);
    opened();

    expect(screen.queryByText(/Copy path \(Shift\+Alt\+C\)/)).toBeNull();
    await userEvent.keyboard("login");
    await screen.findByRole("listbox", { name: "Files" });

    expect(screen.getByText(/Copy path \(Shift\+Alt\+C\)/).textContent).toBe(
      "Copy path (Shift+Alt+C) · Reveal in Files (Shift+Alt+R)",
    );
  });

  it("leave the keys alone with no file aimed", async () => {
    const asked = core([]);
    opened();
    await userEvent.keyboard("zzz");
    await screen.findByText("No file matches what you typed.");

    await userEvent.keyboard("{Shift>}{Alt>}c{/Alt}{/Shift}");

    expect(asked).toEqual([]);
  });
});

describe("the file keys", () => {
  const press = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);

  it("are ⌥⌘C and ⌥⌘R on a Mac, read by the key's place when Option types a symbol", () => {
    expect(fileKeyOf(press({ key: "ç", code: "KeyC", metaKey: true, altKey: true }), true)).toBe(
      "copy",
    );
    expect(fileKeyOf(press({ key: "®", code: "KeyR", metaKey: true, altKey: true }), true)).toBe(
      "reveal",
    );
    // ⌘C is the box's own copy, and Ctrl is the terminal's.
    expect(fileKeyOf(press({ key: "c", code: "KeyC", metaKey: true }), true)).toBeUndefined();
    expect(
      fileKeyOf(press({ key: "c", code: "KeyC", ctrlKey: true, altKey: true }), true),
    ).toBeUndefined();
  });

  it("are Shift+Alt+C and Shift+Alt+R everywhere else, and a letter wins over a place", () => {
    expect(fileKeyOf(press({ key: "C", code: "KeyC", shiftKey: true, altKey: true }), false)).toBe(
      "copy",
    );
    expect(fileKeyOf(press({ key: "R", code: "KeyR", shiftKey: true, altKey: true }), false)).toBe(
      "reveal",
    );
    // Dvorak: the key in C's place types J, which is not this key.
    expect(
      fileKeyOf(press({ key: "J", code: "KeyC", shiftKey: true, altKey: true }), false),
    ).toBeUndefined();
    // AltGr is Ctrl+Alt on Windows: never this key.
    expect(
      fileKeyOf(
        press({ key: "C", code: "KeyC", shiftKey: true, altKey: true, ctrlKey: true }),
        false,
      ),
    ).toBeUndefined();
  });

  it("are read on a Mac from the box too", async () => {
    vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
    const asked = core([file("src/login.ts")]);
    opened();
    await userEvent.keyboard("login");
    await screen.findByRole("listbox", { name: "Files" });

    fireEvent.keyDown(screen.getByRole("combobox"), {
      key: "ç",
      code: "KeyC",
      metaKey: true,
      altKey: true,
    });

    await vi.waitFor(() => expect(asked.map((one) => one.cmd)).toEqual(["copy_branch_path"]));
    expect(screen.getByText(/Copy path \(⌥⌘C\)/).textContent).toBe(
      "Copy path (⌥⌘C) · Reveal in Finder (⌥⌘R)",
    );
  });
});
