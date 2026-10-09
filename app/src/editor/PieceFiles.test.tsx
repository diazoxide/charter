import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { ChangeMark, FolderEntry, PieceFile, PlaneId } from "../bindings";
import { PieceFileTab, PieceFilesTab } from "./PieceFiles";
import { jumpTo } from "../fileJump";
import { forgetYourEditor, setYourEditor } from "../yourEditor";
import type { Offer } from "../actions";

const PLANE = "/plane" as unknown as PlaneId;
const CUT = { workspace: "alpha", repo: "svc", piece: "fix-it" };

const file = (name: string): FolderEntry => ({
  name,
  kind: "file",
  ignored: false,
  refused: null,
});
const folder = (name: string): FolderEntry => ({ ...file(name), kind: "folder" });

/**
 * The core, as these tabs ask it: each file by its path, and each folder of the branch by its
 * path — by default, the folders the files' paths name.
 */
function core(
  files: Record<string, PieceFile | string>,
  editorSays?: string,
  folders?: Record<string, FolderEntry[]>,
  changed: Record<string, ChangeMark> = {},
) {
  const asked: string[] = [];
  const tree = folders ?? foldersOf(Object.keys(files));
  mockIPC(
    (cmd, args) => {
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
