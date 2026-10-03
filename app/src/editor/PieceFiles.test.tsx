import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { PieceFile, PlaneId } from "../bindings";
import { PieceFileTab, PieceFilesTab } from "./PieceFiles";
import { forgetYourEditor, setYourEditor } from "../yourEditor";

const PLANE = "/plane" as unknown as PlaneId;
const CUT = { workspace: "alpha", repo: "svc", piece: "fix-it" };

/** The core, as these tabs ask it: a piece with three files. */
function core(files: Record<string, PieceFile | string>, editorSays?: string) {
  const asked: string[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "open_in_your_editor") {
      const a = args as Record<string, unknown>;
      asked.push(`${cmd}:${a.workspace}/${a.repo}/${a.piece}:${a.path}:${a.line}:${a.editor}`);
      if (editorSays !== undefined) throw editorSays;
      return null;
    }
    const a = args as Record<string, string>;
    if (cmd === "piece_files") {
      asked.push(`${cmd}:${a.workspace}/${a.repo}/${a.piece}`);
      return Object.keys(files);
    }
    if (cmd === "piece_file") {
      asked.push(`${cmd}:${a.path}`);
      const found = files[a.path];
      if (typeof found === "string") throw found;
      return found;
    }
    throw new Error(`unexpected ${cmd}`);
  });
  return asked;
}

afterEach(() => {
  cleanup();
  clearMocks();
  forgetYourEditor();
});

describe("a piece's files", () => {
  it("lists the piece's files, and opens any of them in the light editor", async () => {
    const asked = core({
      "README.md": { kind: "text", text: "# svc\n" },
      "src/lib.rs": { kind: "text", text: "pub fn one() {}\n" },
    });
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);

    await userEvent.click(await screen.findByRole("button", { name: "src/lib.rs" }));

    await waitFor(() =>
      expect(screen.getByTestId("light-editor")).toHaveTextContent("pub fn one() {}"),
    );
    expect(asked).toEqual(["piece_files:alpha/svc/fix-it", "piece_file:src/lib.rs"]);
  });

  it("narrows the list to the files whose path holds what is typed", async () => {
    core({
      "README.md": { kind: "text", text: "" },
      "src/lib.rs": { kind: "text", text: "" },
      "src/main.rs": { kind: "text", text: "" },
    });
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);
    await screen.findByRole("button", { name: "README.md" });

    await userEvent.type(screen.getByRole("searchbox", { name: "Find a file" }), "main");

    expect(screen.queryByRole("button", { name: "README.md" })).toBeNull();
    expect(screen.getByRole("button", { name: "src/main.rs" })).toBeInTheDocument();
  });

  it("opens the file it shows in a tab of its own", async () => {
    core({ "src/lib.rs": { kind: "text", text: "x\n" } });
    const opened = vi.fn();
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={opened} />);
    await userEvent.click(await screen.findByRole("button", { name: "src/lib.rs" }));

    await userEvent.click(await screen.findByRole("button", { name: "Open in a tab of its own" }));

    expect(opened).toHaveBeenCalledWith(
      { from: null, view: "piece-file", key: "alpha/svc/fix-it/src/lib.rs" },
      "lib.rs · fix-it",
    );
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
      await screen.findByText(/big\.log is 6 MiB, past what the light editor draws/),
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

    expect(
      await screen.findByText(/Choose your editor on the Preferences tab/),
    ).toBeInTheDocument();
    expect(asked.some((one) => one.startsWith("open_in_your_editor"))).toBe(false);
  });

  it("says the core's sentence when the editor is not opened", async () => {
    setYourEditor("variable");
    core(
      { "a.txt": { kind: "text", text: "hello\n" } },
      "neither $VISUAL nor $EDITOR is set where charter was started",
    );
    render(<PieceFileTab plane={PLANE} cut={CUT} path="a.txt" />);

    await userEvent.click(await button());

    expect(await screen.findByText(/neither \$VISUAL nor \$EDITOR is set/)).toBeInTheDocument();
  });

  it("is offered beside the file the list shows, too", async () => {
    setYourEditor("zed");
    const asked = core({ "src/lib.rs": { kind: "text", text: "x\n" } });
    render(<PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />);
    await userEvent.click(await screen.findByRole("button", { name: "src/lib.rs" }));

    await userEvent.click(await button());

    await waitFor(() =>
      expect(asked).toContain("open_in_your_editor:alpha/svc/fix-it:src/lib.rs:1:zed"),
    );
  });
});
