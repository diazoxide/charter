import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { BranchChanged, ChangeMark, FolderEntry, PieceFile, PlaneId } from "../bindings";
import { PieceFileTab, PieceFilesTab } from "./PieceFiles";
import { jumpTo } from "../fileJump";
import { forgetYourEditor, setYourEditor } from "../yourEditor";
import { SETTINGS_LINK, type SettingsLinkAsk } from "../settings/links";
import { REVEAL_SAID, type Offer } from "../actions";
import { forgetLastRead, KEPT, lastRead, readAt } from "./lastRead";
import { forgetSvgProbe } from "./imagePreview";
import { forgetShown, windowShown } from "../test-shown";
import { REDUCE } from "../theme/motion";

const PLANE = "/plane" as unknown as PlaneId;
const CUT = { workspace: "alpha", repo: "svc", piece: "fix-it" };

const file = (name: string): FolderEntry => ({
  name,
  kind: "file",
  ignored: false,
  refused: null,
});
const folder = (name: string): FolderEntry => ({ ...file(name), kind: "folder" });

/** Each `branch_status` the tabs asked, by branch, since the last `core`. */
let statusAsks: string[] = [];

/**
 * The core, as these tabs ask it: each file by its path, and each folder of the branch by its
 * path — by default, the folders the files' paths name.
 */
function core(
  files: Record<string, PieceFile | string>,
  editorSays?: string,
  folders?: Record<string, FolderEntry[]>,
  changed: Record<string, ChangeMark> = {},
  placeSays?: string,
) {
  const asked: string[] = [];
  statusAsks = [];
  const tree = folders ?? foldersOf(Object.keys(files));
  mockIPC(
    (cmd, args) => {
      if (cmd === "copy_branch_path" || cmd === "reveal_branch_path") {
        const a = args as Record<string, unknown>;
        const absolute = cmd === "copy_branch_path" ? `:${String(a.absolute)}` : "";
        asked.push(`${cmd}:${a.workspace}/${a.repo}/${a.piece}:${a.path}${absolute}`);
        if (placeSays !== undefined) throw placeSays;
        return null;
      }
      if (cmd === "open_in_your_editor") {
        const a = args as Record<string, unknown>;
        asked.push(`${cmd}:${a.workspace}/${a.repo}/${a.piece}:${a.path}:${a.line}:${a.editor}`);
        if (editorSays !== undefined) throw editorSays;
        return null;
      }
      const a = args as Record<string, string>;
      if (cmd === "branch_tree") {
        asked.push(`${cmd}:${a.workspace}/${a.repo}/${a.piece}:${a.folder}`);
        return { entries: tree[a.folder] ?? [], more: 0 };
      }
      if (cmd === "files_watch") return null;
      if (cmd === "branch_status") {
        statusAsks.push(`${a.workspace}/${a.repo}/${a.piece}`);
        return {
          changes: Object.entries(changed).map(([path, mark]) => ({
            path,
            mark,
            from: null,
            uncommitted: true,
          })),
          folders: [],
          more: 0,
          base: "main",
        };
      }
      if (cmd === "piece_file") {
        asked.push(`${cmd}:${a.path}`);
        const found = files[a.path];
        if (typeof found === "string") throw found;
        return found;
      }
      throw new Error(`unexpected ${cmd}`);
    },
    { shouldMockEvents: true },
  );
  return asked;
}

/** The folders a set of paths makes, each folder's folders first. */
function foldersOf(paths: string[]): Record<string, FolderEntry[]> {
  const out: Record<string, FolderEntry[]> = {};
  const add = (at: string, entry: FolderEntry) => {
    const list = (out[at] ??= []);
    if (!list.some((one) => one.name === entry.name)) list.push(entry);
  };
  for (const path of paths) {
    const parts = path.split("/");
    parts.forEach((name, i) => {
      const at = parts.slice(0, i).join("/");
      add(at, i === parts.length - 1 ? file(name) : folder(name));
    });
  }
  for (const list of Object.values(out))
    list.sort((a, b) => (a.kind === b.kind ? 0 : a.kind === "folder" ? -1 : 1));
  return out;
}

afterEach(() => {
  cleanup();
  clearMocks();
  forgetYourEditor();
  forgetLastRead();
  forgetSvgProbe();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

/** The tab's tree, and a row of it by name. */
const tree = () => screen.findByRole("tree", { name: "Files of fix-it" });
const row = async (name: string, timeout?: number) =>
  within(await tree()).findByRole("treeitem", { name }, { timeout });

describe("a branch's files", () => {
  it("draws the branch as a tree, folder by folder, and opens a file in the preview", async () => {
    const asked = core({
      "README.md": { kind: "text", text: "# svc\n" },
      "src/lib.rs": { kind: "text", text: "pub fn one() {}\n" },
    });
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);

    expect(await row("src")).toHaveAttribute("aria-expanded", "false");
    expect(within(await tree()).queryByRole("treeitem", { name: "lib.rs" })).toBeNull();
    await userEvent.click(await row("src"));
    await userEvent.click(await row("lib.rs"));

    await waitFor(() =>
      expect(screen.getByTestId("light-editor")).toHaveTextContent("pub fn one() {}"),
    );
    expect(await row("lib.rs")).toHaveAttribute("aria-selected", "true");
    expect(asked).toEqual([
      "branch_tree:alpha/svc/fix-it:",
      "branch_tree:alpha/svc/fix-it:src",
      "piece_file:src/lib.rs",
    ]);
  });

  it("draws each row with its file-type or folder icon (FM-3)", async () => {
    core({
      "README.md": { kind: "text", text: "# svc\n" },
      "src/lib.rs": { kind: "text", text: "pub fn one() {}\n" },
    });
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);
    const icon = async (name: string) =>
      (await row(name)).querySelector("svg.file-icon")?.getAttribute("data-icon");

    expect(await icon("src")).toBe("folder-src");
    expect(await icon("README.md")).toBe("readme");
    await userEvent.click(await row("src"));
    expect(await icon("lib.rs")).toBe("rust");
  });

  it("draws every entry of a folder, with no cap of its own", async () => {
    const many = Object.fromEntries(
      Array.from({ length: 600 }, (_, i) => [`f${i}.txt`, { kind: "text", text: "" } as PieceFile]),
    );
    core(many);
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);

    await row("f599.txt", 20_000);

    expect(within(await tree()).getAllByRole("treeitem")).toHaveLength(600);
    // Six hundred rows through jsdom take seconds on a busy runner; the default 5 is for one.
  }, 30_000);

  it("moves through the tree with the arrows, and opens a folder with Right", async () => {
    core({ "a.txt": { kind: "text", text: "" }, "src/lib.rs": { kind: "text", text: "" } });
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);
    (await row("src")).focus();

    await userEvent.keyboard("{ArrowRight}");
    expect(await row("src")).toHaveAttribute("aria-expanded", "true");
    await userEvent.keyboard("{ArrowRight}");

    expect(await row("lib.rs")).toHaveFocus();
  });

  it("opens the file it shows in a tab of its own", async () => {
    core({ "src/lib.rs": { kind: "text", text: "x\n" } });
    const opened = vi.fn();
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={opened} />);
    await userEvent.click(await row("src"));
    await userEvent.click(await row("lib.rs"));

    await userEvent.click(await screen.findByRole("button", { name: "Open in a tab of its own" }));

    expect(opened).toHaveBeenCalledWith(
      { from: null, view: "piece-file", key: "alpha/svc/fix-it/src/lib.rs" },
      "lib.rs · fix-it",
    );
  });
});

describe("Show what changed (FM-11)", () => {
  it("is offered for a file the branch changed, and opens its comparison as a view tab", async () => {
    core(
      {
        "notes.txt": { kind: "text", text: "plain notes\n" },
        "src/lib.rs": { kind: "text", text: "pub fn one() {}\n" },
      },
      undefined,
      undefined,
      { "src/lib.rs": "changed" },
    );
    const opened = vi.fn();
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={opened} />);
    await userEvent.click(await row("src"));
    await userEvent.click(await row("lib.rs"));

    await userEvent.click(await screen.findByRole("button", { name: "Show what changed" }));

    expect(opened).toHaveBeenCalledWith(
      { from: null, view: "piece-diff", key: "alpha/svc/fix-it/src/lib.rs" },
      "What changed · lib.rs · fix-it",
    );
  });

  it("is absent for a file the branch did not change", async () => {
    core(
      {
        "notes.txt": { kind: "text", text: "plain notes\n" },
        "src/lib.rs": { kind: "text", text: "pub fn one() {}\n" },
      },
      undefined,
      undefined,
      { "src/lib.rs": "changed" },
    );
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);
    await userEvent.click(await row("src"));
    await userEvent.click(await row("lib.rs"));
    await screen.findByRole("button", { name: "Show what changed" });

    await userEvent.click(await row("notes.txt"));

    await waitFor(() =>
      expect(screen.getByTestId("light-editor")).toHaveTextContent("plain notes"),
    );
    expect(screen.queryByRole("button", { name: "Show what changed" })).toBeNull();
  });

  it("is absent when what the branch changed could not be read", async () => {
    mockIPC(
      (cmd, args) => {
        const a = args as Record<string, string>;
        if (cmd === "branch_tree")
          return { entries: a.folder === "" ? [file("notes.txt")] : [], more: 0 };
        if (cmd === "files_watch") return null;
        if (cmd === "piece_file") return { kind: "text", text: "plain notes\n" };
        if (cmd === "branch_status") throw "charter could not read the branch";
        throw new Error(`unexpected ${cmd}`);
      },
      { shouldMockEvents: true },
    );
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);
    await userEvent.click(await row("notes.txt"));

    await waitFor(() =>
      expect(screen.getByTestId("light-editor")).toHaveTextContent("plain notes"),
    );
    expect(screen.queryByRole("button", { name: "Show what changed" })).toBeNull();
  });
});

describe("Show what changed in a file's own tab (#1189)", () => {
  it("is offered for a file the branch changed, and opens its comparison", async () => {
    core({ "src/lib.rs": { kind: "text", text: "x\n" } }, undefined, undefined, {
      "src/lib.rs": "changed",
    });
    const opened = vi.fn();
    render(<PieceFileTab plane={PLANE} cut={CUT} path="src/lib.rs" onOpenView={opened} />);

    await userEvent.click(await screen.findByRole("button", { name: "Show what changed" }));

    expect(opened).toHaveBeenCalledWith(
      { from: null, view: "piece-diff", key: "alpha/svc/fix-it/src/lib.rs" },
      "What changed · lib.rs · fix-it",
    );
  });

  it("is absent for a file the branch did not change", async () => {
    core(
      { "a.txt": { kind: "text", text: "hello\n" }, "b.txt": { kind: "text", text: "" } },
      undefined,
      undefined,
      {
        "b.txt": "added",
      },
    );
    render(<PieceFileTab plane={PLANE} cut={CUT} path="a.txt" onOpenView={() => undefined} />);

    await waitFor(() => expect(screen.getByTestId("light-editor")).toHaveTextContent("hello"));
    expect(screen.queryByRole("button", { name: "Show what changed" })).toBeNull();
  });

  const moved = (piece: string | null, workspace = "alpha") =>
    act(() =>
      emit("branch-changed", {
        branches: [{ plane: PLANE, workspace, repo: "svc", piece }],
      } satisfies BranchChanged),
    );

  it("reads what the branch changed again when its own branch moves", async () => {
    const marks: Record<string, ChangeMark> = {};
    core({ "src/lib.rs": { kind: "text", text: "x\n" } }, undefined, undefined, marks);
    render(<PieceFileTab plane={PLANE} cut={CUT} path="src/lib.rs" onOpenView={() => undefined} />);
    await waitFor(() => expect(screen.getByTestId("light-editor")).toHaveTextContent("x"));
    expect(screen.queryByRole("button", { name: "Show what changed" })).toBeNull();

    marks["src/lib.rs"] = "changed";
    await moved("fix-it");
    expect(await screen.findByRole("button", { name: "Show what changed" })).toBeInTheDocument();

    delete marks["src/lib.rs"];
    await moved("fix-it");
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Show what changed" })).toBeNull(),
    );
  });

  it("does not read again when another branch moves", async () => {
    const marks: Record<string, ChangeMark> = {};
    core({ "src/lib.rs": { kind: "text", text: "x\n" } }, undefined, undefined, marks);
    render(<PieceFileTab plane={PLANE} cut={CUT} path="src/lib.rs" onOpenView={() => undefined} />);
    await waitFor(() => expect(statusAsks).toEqual(["alpha/svc/fix-it"]));

    marks["src/lib.rs"] = "changed";
    await moved("other");
    await moved(null);
    await moved("fix-it", "beta");
    await act(async () => {
      await new Promise((done) => setTimeout(done, 20));
    });

    expect(statusAsks).toEqual(["alpha/svc/fix-it"]);
    expect(screen.queryByRole("button", { name: "Show what changed" })).toBeNull();
  });
});

describe("the preview (FM-2)", () => {
  /** Opens `path` from the tab's tree, which lists exactly it. */
  async function preview(path: string, answer: PieceFile) {
    core({ [path]: answer });
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);
    await userEvent.click(await row(path));
  }

  it("renders a markdown file, and shows its source on request", async () => {
    await preview("NOTES.md", { kind: "text", text: "# Plan\n\nShip *it*.\n" });

    expect(await screen.findByRole("heading", { name: "Plan" })).toBeInTheDocument();
    expect(screen.queryByTestId("light-editor")).toBeNull();

    await userEvent.click(screen.getByRole("button", { name: "Source" }));

    await waitFor(() => expect(screen.getByTestId("light-editor")).toHaveTextContent("# Plan"));
    expect(screen.getByRole("button", { name: "Source" })).toHaveAttribute("aria-pressed", "true");
  });

  it("draws an image as an image, decoded in the window", async () => {
    const drawn = vi.fn();
    vi.stubGlobal(
      "createImageBitmap",
      vi.fn(async () => ({ width: 3, height: 2, close: () => undefined })),
    );
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
      drawImage: drawn,
    } as unknown as CanvasRenderingContext2D);

    await preview("logo.png", { kind: "image", mime: "image/png", base64: "iVBORw0KGgo=" });

    expect(await screen.findByRole("img", { name: "logo.png" })).toBeInTheDocument();
    expect(await screen.findByText("3 × 2 · image/png")).toBeInTheDocument();
    expect(drawn).toHaveBeenCalled();
  });

  it("draws an SVG as an image decoded from its bytes, with its text a press away (#1132)", async () => {
    const decoded: Blob[] = [];
    const drawn = vi.fn();
    vi.stubGlobal(
      "createImageBitmap",
      vi.fn(async (blob: Blob) => {
        decoded.push(blob);
        return { width: 24, height: 16, close: () => undefined };
      }),
    );
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
      drawImage: drawn,
    } as unknown as CanvasRenderingContext2D);
    const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="16"/>';

    await preview("icon.svg", { kind: "text", text: svg });

    expect(await screen.findByRole("img", { name: "icon.svg" })).toBeInTheDocument();
    expect(await screen.findByText("24 × 16 · image/svg+xml")).toBeInTheDocument();
    expect(drawn).toHaveBeenCalled();
    expect(screen.queryByTestId("light-editor")).toBeNull();
    // Handed to the decoder as bytes of an image, never put into the page as markup.
    const file = decoded.at(-1);
    expect(file?.type).toBe("image/svg+xml");
    expect(await file?.text()).toBe(svg);
    expect(document.querySelector("svg[width='24']")).toBeNull();

    await userEvent.click(screen.getByRole("button", { name: "Source" }));

    await waitFor(() => expect(screen.getByTestId("light-editor")).toHaveTextContent("<svg"));
    expect(screen.queryByRole("img", { name: "icon.svg" })).toBeNull();
  });

  it("shows an SVG as its text where the window does not decode SVG from bytes", async () => {
    vi.stubGlobal(
      "createImageBitmap",
      vi.fn(async () => {
        throw new DOMException("The source image could not be decoded.", "InvalidStateError");
      }),
    );

    await preview("icon.svg", { kind: "text", text: "<svg/>" });

    await waitFor(() => expect(screen.getByTestId("light-editor")).toHaveTextContent("<svg/>"));
    expect(screen.queryByRole("img", { name: "icon.svg" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Source" })).toBeNull();
  });

  it("says an SVG declaring a huge canvas by its size, and decodes none of it", async () => {
    const decode = vi.fn(async (blob: Blob) => {
      if ((await blob.text()).includes("40000")) throw new Error("decoded the huge one");
      return { width: 1, height: 1, close: () => undefined };
    });
    vi.stubGlobal("createImageBitmap", decode);

    await preview("map.svg", {
      kind: "text",
      text: '<?xml version="1.0"?>\n<svg width="40000px" height="40000" viewBox="0 0 1 1"/>',
    });

    expect(
      await screen.findByText(
        "map.svg is 40000 × 40000 pixels, past what the preview draws (40 megapixels)",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Source" })).toBeInTheDocument();
  });

  it("says an image declaring a huge canvas by its size, and draws none of it", async () => {
    await preview("bomb.png", {
      kind: "huge-image",
      mime: "image/png",
      width: 16000,
      height: 16000,
    });

    expect(
      await screen.findByText(
        "bomb.png is 16000 × 16000 pixels, past what the preview draws (40 megapixels)",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("img")).toBeNull();
  });

  it("says a binary file is binary, and draws none of it", async () => {
    await preview("tool.bin", { kind: "binary", bytes: 2048 });

    expect(await screen.findByText(/tool\.bin is a binary file \(2 KiB\)/)).toBeInTheDocument();
    expect(screen.queryByTestId("light-editor")).toBeNull();
  });

  it("says a file past 2 MiB is too large to preview, and offers your editor", async () => {
    await preview("big.log", { kind: "too-large", bytes: 3 * 1024 * 1024 });

    expect(
      await screen.findByText("big.log is 3 MiB, past what the preview draws (2 MiB)"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Open in your editor/ })).toBeInTheDocument();
  });
});

describe("an animated image in the preview (#1132)", () => {
  /** A GIF of two 1×1 frames: animated by its bytes. */
  const SPINNER =
    "R0lGODlhAwACAIAAAAAAAP///yH5BAAKAAAALAAAAAABAAEAAAICTAEAIfkEAAoAAAAsAAAAAAEAAQAAAgJMAQA7";
  const READ: PieceFile = { kind: "image", mime: "image/gif", base64: SPINNER };

  /** What the canvas drew, in order: a frame's index, or "still" for the first decode. */
  let drawn: (number | "still")[] = [];

  function canvasAndStill() {
    drawn = [];
    vi.stubGlobal(
      "createImageBitmap",
      vi.fn(async () => ({ width: 3, height: 2, close: () => undefined })),
    );
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
      drawImage: (image: { frame?: number }) => drawn.push(image.frame ?? "still"),
    } as unknown as CanvasRenderingContext2D);
  }

  /** A webview with WebCodecs' `ImageDecoder`: `frames` frames of 20 ms each, looping. */
  function decoder(frames = 3) {
    const made: { data: unknown; type: string }[] = [];
    const closed = vi.fn();
    class Decoder {
      static isTypeSupported = async () => true;
      tracks = {
        ready: Promise.resolve(),
        selectedTrack: { animated: true, frameCount: frames, repetitionCount: Infinity },
      };
      completed = Promise.resolve();
      constructor(init: { data: unknown; type: string }) {
        made.push(init);
      }
      async decode({ frameIndex = 0 }: { frameIndex?: number } = {}) {
        return { image: { frame: frameIndex, duration: 20_000, close: () => undefined } };
      }
      close = closed;
    }
    vi.stubGlobal("ImageDecoder", Decoder);
    return { made, closed };
  }

  /** Whether an element is in sight, as `IntersectionObserver` tells it. */
  function sight() {
    const told: ((entries: { isIntersecting: boolean }[]) => void)[] = [];
    vi.stubGlobal(
      "IntersectionObserver",
      class {
        constructor(callback: (entries: { isIntersecting: boolean }[]) => void) {
          told.push(callback);
        }
        observe() {}
        disconnect() {}
      },
    );
    return (seen: boolean) => act(() => told.forEach((tell) => tell([{ isIntersecting: seen }])));
  }

  const reduceMotion = () =>
    vi.stubGlobal("matchMedia", (query: string) => ({
      matches: query === REDUCE,
      media: query,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    }));

  const quiet = (ms: number) => new Promise((done) => setTimeout(done, ms));

  async function preview(path: string) {
    core({ [path]: READ });
    const shown = render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);
    await userEvent.click(await row(path));
    return shown;
  }

  afterEach(() => forgetShown());

  it("plays frame by frame on the canvas, decoded from its bytes, and loops", async () => {
    canvasAndStill();
    const { made } = decoder(3);

    await preview("spin.gif");

    await waitFor(() => expect(drawn.join()).toContain("0,1,2,0"));
    expect(made[0]?.type).toBe("image/gif");
    expect(made[0]?.data).toBeInstanceOf(Uint8Array);
    expect(screen.getByRole("button", { name: "Pause" })).toBeInTheDocument();
  });

  it("stops while its tab is out of sight or the window hidden, and lets its decoder go when closed", async () => {
    canvasAndStill();
    const { closed } = decoder(3);
    const seen = sight();

    const { unmount } = await preview("spin.gif");
    await waitFor(() => expect(drawn.length).toBeGreaterThan(3));

    seen(false);
    let held = drawn.length;
    await quiet(120);
    expect(drawn.length).toBe(held);

    seen(true);
    await waitFor(() => expect(drawn.length).toBeGreaterThan(held + 2));

    act(() => windowShown(false));
    held = drawn.length;
    await quiet(120);
    expect(drawn.length).toBe(held);

    act(() => windowShown(true));
    await waitFor(() => expect(drawn.length).toBeGreaterThan(held + 2));

    unmount();
    expect(closed).toHaveBeenCalled();
  });

  it("shows one frame and Play under reduced motion, and plays only when asked", async () => {
    canvasAndStill();
    decoder(3);
    reduceMotion();

    await preview("spin.gif");

    const play = await screen.findByRole("button", { name: "Play" });
    await quiet(120);
    expect(drawn).toEqual(["still"]);

    await userEvent.click(play);

    await waitFor(() => expect(drawn).toEqual(expect.arrayContaining([1, 2])));
    await userEvent.click(screen.getByRole("button", { name: "Pause" }));
    const held = drawn.length;
    await quiet(120);
    expect(drawn.length).toBe(held);
  });

  it("shows its first frame and says so where the window has no decoder for frames", async () => {
    canvasAndStill();

    await preview("spin.gif");

    expect(
      await screen.findByText(
        "3 × 2 · image/gif · its first frame: this window does not play animation",
      ),
    ).toBeInTheDocument();
    expect(drawn).toEqual(["still"]);
    expect(screen.queryByRole("button", { name: "Play" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Pause" })).toBeNull();
  });
});

describe("one file of a piece", () => {
  it("draws a text file", async () => {
    core({ "a.txt": { kind: "text", text: "hello\n" } });

    render(<PieceFileTab plane={PLANE} cut={CUT} path="a.txt" />);

    await waitFor(() => expect(screen.getByTestId("light-editor")).toHaveTextContent("hello"));
  });

  it("says a binary file is binary, and draws none of it", async () => {
    core({ "logo.png": { kind: "binary", bytes: 2048 } });

    render(<PieceFileTab plane={PLANE} cut={CUT} path="logo.png" />);

    expect(await screen.findByText(/logo\.png is a binary file \(2 KiB\)/)).toBeInTheDocument();
    expect(screen.queryByTestId("light-editor")).toBeNull();
  });

  it("offers a file past the light editor's size to your editor", async () => {
    core({ "big.log": { kind: "too-large", bytes: 6 * 1024 * 1024 } });

    render(<PieceFileTab plane={PLANE} cut={CUT} path="big.log" />);

    expect(
      await screen.findByText(/big\.log is 6 MiB, past what the preview draws/),
    ).toBeInTheDocument();
  });

  it("says the core's sentence when the file does not open", async () => {
    core({ gone: "'gone' is not in the branch's folder any more" });

    render(<PieceFileTab plane={PLANE} cut={CUT} path="gone" />);

    expect(
      await screen.findByText("'gone' is not in the branch's folder any more"),
    ).toBeInTheDocument();
  });
});

describe("open in your editor (RC-20)", () => {
  const button = () => screen.findByRole("button", { name: /Open in your editor/ });

  it("hands the file and the line it was opened at to the editor chosen", async () => {
    setYourEditor("zed");
    const asked = core({ "src/lib.rs": { kind: "text", text: "a\nb\nc\n" } });
    render(<PieceFileTab plane={PLANE} cut={CUT} path="src/lib.rs" line={2} />);
    await waitFor(() => expect(screen.getByTestId("light-editor")).toHaveTextContent("a"));

    await userEvent.click(await button());

    await waitFor(() =>
      expect(asked).toContain("open_in_your_editor:alpha/svc/fix-it:src/lib.rs:2:zed"),
    );
  });

  it("opens at the first line when no line was asked for", async () => {
    setYourEditor("vscode");
    const asked = core({ "a.txt": { kind: "text", text: "hello\n" } });
    render(<PieceFileTab plane={PLANE} cut={CUT} path="a.txt" />);

    await userEvent.click(await button());

    await waitFor(() =>
      expect(asked).toContain("open_in_your_editor:alpha/svc/fix-it:a.txt:1:vscode"),
    );
  });

  it("offers a file past the light editor's size to your editor, at its first line", async () => {
    setYourEditor("idea");
    const asked = core({ "big.log": { kind: "too-large", bytes: 6 * 1024 * 1024 } });
    render(<PieceFileTab plane={PLANE} cut={CUT} path="big.log" />);

    await userEvent.click(await button());

    await waitFor(() =>
      expect(asked).toContain("open_in_your_editor:alpha/svc/fix-it:big.log:1:idea"),
    );
  });

  it("asks for an editor when none is chosen, and hands nothing on", async () => {
    const asked = core({ "a.txt": { kind: "text", text: "hello\n" } });
    render(<PieceFileTab plane={PLANE} cut={CUT} path="a.txt" />);

    await userEvent.click(await button());

    expect(await screen.findByText(/Choose your editor in Settings/)).toBeInTheDocument();
    expect(asked.some((one) => one.startsWith("open_in_your_editor"))).toBe(false);
  });

  it("links the ask for an editor to Settings' Editor group (#1201)", async () => {
    core({ "a.txt": { kind: "text", text: "hello\n" } });
    const asked: SettingsLinkAsk[] = [];
    const heard = (event: Event) => asked.push((event as CustomEvent<SettingsLinkAsk>).detail);
    window.addEventListener(SETTINGS_LINK, heard);
    try {
      render(<PieceFileTab plane={PLANE} cut={CUT} path="a.txt" />);
      await userEvent.click(await button());

      await userEvent.click(await screen.findByRole("button", { name: "Choose your editor" }));

      expect(asked).toEqual([
        { plane: PLANE, link: { group: "you.editor", setting: "you.editor.yours" } },
      ]);
    } finally {
      window.removeEventListener(SETTINGS_LINK, heard);
    }
  });

  it("says the core's sentence when the editor is not opened", async () => {
    setYourEditor("variable");
    core(
      { "a.txt": { kind: "text", text: "hello\n" } },
      "neither $VISUAL nor $EDITOR is set where purlis was started",
    );
    render(<PieceFileTab plane={PLANE} cut={CUT} path="a.txt" />);

    await userEvent.click(await button());

    expect(await screen.findByText(/neither \$VISUAL nor \$EDITOR is set/)).toBeInTheDocument();
  });

  it("is offered beside the file the tree shows, too", async () => {
    setYourEditor("zed");
    const asked = core({ "src/lib.rs": { kind: "text", text: "x\n" } });
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);
    await userEvent.click(await row("src"));
    await userEvent.click(await row("lib.rs"));

    await userEvent.click(await button());

    await waitFor(() =>
      expect(asked).toContain("open_in_your_editor:alpha/svc/fix-it:src/lib.rs:1:zed"),
    );
  });
});

describe("Copy path and Reveal in the preview's header (#1143)", () => {
  it("copies the file's path in the branch, and says so", async () => {
    const asked = core({ "src/lib.rs": { kind: "text", text: "x\n" } });
    render(<PieceFileTab plane={PLANE} cut={CUT} path="src/lib.rs" />);

    await userEvent.click(await screen.findByRole("button", { name: "Copy path" }));

    expect(await screen.findByText("Copied the path of src/lib.rs.")).toBeInTheDocument();
    expect(asked).toContain("copy_branch_path:alpha/svc/fix-it:src/lib.rs:false");
  });

  it("reveals the file where the platform's file manager shows it", async () => {
    const asked = core({ "src/lib.rs": { kind: "text", text: "x\n" } });
    render(<PieceFileTab plane={PLANE} cut={CUT} path="src/lib.rs" />);

    await userEvent.click(await screen.findByRole("button", { name: REVEAL_SAID }));

    await waitFor(() => expect(asked).toContain("reveal_branch_path:alpha/svc/fix-it:src/lib.rs"));
  });

  it("says the core's sentence when it refuses", async () => {
    core(
      { "a.txt": { kind: "text", text: "x\n" } },
      undefined,
      undefined,
      {},
      "purlis follows no link",
    );
    render(<PieceFileTab plane={PLANE} cut={CUT} path="a.txt" />);

    await userEvent.click(await screen.findByRole("button", { name: REVEAL_SAID }));

    expect(await screen.findByRole("status")).toHaveTextContent("purlis follows no link");
  });

  it("is offered beside the file the tree shows, too", async () => {
    const asked = core({ "src/lib.rs": { kind: "text", text: "x\n" } });
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);
    await userEvent.click(await row("src"));
    await userEvent.click(await row("lib.rs"));

    await userEvent.click(await screen.findByRole("button", { name: "Copy path" }));
    await userEvent.click(await screen.findByRole("button", { name: REVEAL_SAID }));

    await waitFor(() =>
      expect(asked).toEqual(
        expect.arrayContaining([
          "copy_branch_path:alpha/svc/fix-it:src/lib.rs:false",
          "reveal_branch_path:alpha/svc/fix-it:src/lib.rs",
        ]),
      ),
    );
  });
});

describe("a row of the file tab's tree (FM-10)", () => {
  it("has the explorer's menu: its paths, a reveal, and your editor or a shell there", async () => {
    core({ "README.md": { kind: "text", text: "x" }, "src/lib.rs": { kind: "text", text: "" } });
    const pressed: Offer[] = [];
    render(
      <PieceFilesTab
        plane={PLANE}
        cut={CUT}
        onOpenView={() => undefined}
        onPress={(offer) => pressed.push(offer)}
      />,
    );

    fireEvent.contextMenu(await row("src"));
    const menu = await screen.findByRole("menu");
    expect(within(menu).getByRole("menuitem", { name: "Open a shell tab here" })).toBeVisible();
    await userEvent.click(within(menu).getByRole("menuitem", { name: "Copy absolute path" }));

    expect(pressed.map((offer) => offer.does)).toEqual([
      { verb: "copyPath", at: { ...CUT, path: "src" }, absolute: true },
    ]);
  });

  it("opens your editor from a file's row at the line its preview was read at (#1143)", async () => {
    core({ "src/lib.rs": { kind: "text", text: "a\nb\nc\n" } });
    const pressed: Offer[] = [];
    jumpTo({ plane: PLANE, place: CUT, path: "src/lib.rs", line: 3 });
    render(
      <PieceFilesTab
        plane={PLANE}
        cut={CUT}
        onOpenView={() => undefined}
        onPress={(offer) => pressed.push(offer)}
      />,
    );
    expect(
      await screen.findByRole("button", { name: "Open in your editor at line 3" }),
    ).toBeInTheDocument();

    fireEvent.contextMenu(await row("lib.rs"));
    const menu = await screen.findByRole("menu");
    await userEvent.click(within(menu).getByRole("menuitem", { name: "Open in your editor" }));

    expect(pressed.map((offer) => offer.does)).toEqual([
      { verb: "openInEditor", at: { ...CUT, path: "src/lib.rs" }, line: 3 },
    ]);
  });
});

describe("the line last read (#1143)", () => {
  it("is kept per file of a branch, from the file's own tab too", async () => {
    core({ "src/lib.rs": { kind: "text", text: "a\nb\nc\n" } });
    render(<PieceFileTab plane={PLANE} cut={CUT} path="src/lib.rs" line={2} />);

    await waitFor(() => expect(lastRead(PLANE, CUT, "src/lib.rs")).toBe(2));
    // Another branch's file of that name, and another project's, were not read.
    expect(lastRead(PLANE, { ...CUT, piece: "other" }, "src/lib.rs")).toBe(1);
    expect(lastRead("/elsewhere", CUT, "src/lib.rs")).toBe(1);
  });

  it("forgets the oldest past what it keeps, and line 1 is no line to keep", () => {
    readAt(PLANE, CUT, "first.rs", 9);
    for (let i = 0; i < KEPT; i++) readAt(PLANE, CUT, `f${i}.rs`, 5);

    expect(lastRead(PLANE, CUT, "first.rs")).toBe(1);
    expect(lastRead(PLANE, CUT, `f${KEPT - 1}.rs`)).toBe(5);
    readAt(PLANE, CUT, "f0.rs", 1);
    expect(lastRead(PLANE, CUT, "f0.rs")).toBe(1);
  });
});

describe("a jump reveals its file in the tree (#1137)", () => {
  /** The rows brought into view, by their `data-row`: jsdom has no layout of its own. */
  function scrolls() {
    const scrolled: string[] = [];
    Object.defineProperty(Element.prototype, "scrollIntoView", {
      configurable: true,
      value(this: Element) {
        scrolled.push(this.getAttribute("data-row") ?? "");
      },
    });
    return scrolled;
  }
  afterEach(() => {
    delete (Element.prototype as unknown as { scrollIntoView?: unknown }).scrollIntoView;
  });

  const LIB = "file:svc/fix-it:src/deep/lib.rs";

  it("opens the folders above the file a jump picked, and scrolls its row into view", async () => {
    const scrolled = scrolls();
    core({
      "README.md": { kind: "text", text: "# svc\n" },
      "src/deep/lib.rs": { kind: "text", text: "a\nb\n" },
    });
    jumpTo({ plane: PLANE, place: CUT, path: "src/deep/lib.rs", line: 2 });
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);

    expect(await row("lib.rs")).toHaveAttribute("aria-selected", "true");
    expect(await row("src")).toHaveAttribute("aria-expanded", "true");
    expect(await row("deep")).toHaveAttribute("aria-expanded", "true");
    await waitFor(() => expect(scrolled).toEqual([LIB]));
  });

  it("reveals the file again on a later jump to it, after its folder was closed", async () => {
    const scrolled = scrolls();
    core({ "src/deep/lib.rs": { kind: "text", text: "a\nb\n" } });
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);
    act(() => jumpTo({ plane: PLANE, place: CUT, path: "src/deep/lib.rs", line: 1 }));
    expect(await row("lib.rs")).toHaveAttribute("aria-selected", "true");
    await waitFor(() => expect(scrolled).toEqual([LIB]));

    await userEvent.click(await row("src"));
    expect(await row("src")).toHaveAttribute("aria-expanded", "false");
    act(() => jumpTo({ plane: PLANE, place: CUT, path: "src/deep/lib.rs", line: 2 }));

    expect(await row("lib.rs")).toHaveAttribute("aria-selected", "true");
    await waitFor(() => expect(scrolled).toEqual([LIB, LIB]));
  });

  it("leaves the folders as they are for a file picked in the tree itself", async () => {
    scrolls();
    core({
      "src/lib.rs": { kind: "text", text: "x\n" },
      "src/more/a.rs": { kind: "text", text: "" },
    });
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);
    await userEvent.click(await row("src"));
    await userEvent.click(await row("lib.rs"));

    expect(await row("lib.rs")).toHaveAttribute("aria-selected", "true");
    expect(await row("more")).toHaveAttribute("aria-expanded", "false");
  });
});
