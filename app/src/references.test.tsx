import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  DragHandle,
  droppedReference,
  handedSaid,
  handReference,
  PickAChat,
  REFERENCE_TYPE,
  ReferenceChats,
  type ChatsForReferences,
  type Referenced,
} from "./references";

/**
 * **Files into chats, as the window draws them** (FM-9): the states e2e does not reach cheaply —
 * the picker with no chat open, a reference copied rather than typed, a drop that is not
 * charter's, a hit from another project. What the core types, and when, is
 * `reference.rs`'s and `references.rs`'s; the drag onto a running chat is the e2e's.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const FILE: Referenced = {
  plane: "/p",
  workspace: "alpha",
  repo: "svc",
  piece: "fix-it",
  path: "src/main.rs",
  folder: false,
};

function lend(chats: ChatsForReferences["chats"]) {
  const hand = vi.fn();
  const value: ChatsForReferences = { plane: "/p", chats, hand };
  return { hand, value };
}

/** A drop event's `dataTransfer`, holding `data` by type. */
function carrying(data: Record<string, string>) {
  return {
    dataTransfer: {
      types: Object.keys(data),
      getData: (type: string) => data[type] ?? "",
    },
  } as unknown as React.DragEvent;
}

describe("the preview's chat picker", () => {
  it("says there is no chat to pick when none is open", () => {
    const { value } = lend([]);
    render(
      <ReferenceChats.Provider value={value}>
        <PickAChat referenced={FILE} how="ask" />
      </ReferenceChats.Provider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Ask a chat about this" }));

    expect(screen.getByRole("status").textContent).toMatch(/No chat is open in this project/);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("lists the project's chats and hands the reference to the one picked", () => {
    const { hand, value } = lend([
      { session: 3, name: "steward 3" },
      { session: 7, name: "fix login" },
    ]);
    render(
      <ReferenceChats.Provider value={value}>
        <PickAChat referenced={FILE} how="add" />
      </ReferenceChats.Provider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Add to a chat's context" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "fix login" }));

    expect(hand).toHaveBeenCalledWith(FILE, 7, "add");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("is not drawn where no window lends it chats", () => {
    render(<PickAChat referenced={FILE} how="ask" />);

    expect(screen.queryByRole("button")).toBeNull();
  });
});

describe("the preview's drag handle", () => {
  it("names the lines selected, and the file when none are", () => {
    const { rerender } = render(<DragHandle referenced={FILE} />);
    expect(screen.getByTestId("reference-handle").textContent).toBe("src/main.rs");

    rerender(<DragHandle referenced={{ ...FILE, lines: { first: 4, last: 9 } }} />);
    expect(screen.getByTestId("reference-handle").textContent).toBe("src/main.rs:4-9");
  });
});

describe("a drop on a chat", () => {
  it("takes only charter's own reference, whole", () => {
    expect(droppedReference(carrying({ "text/plain": "/etc/passwd" }))).toBeUndefined();
    expect(droppedReference(carrying({ [REFERENCE_TYPE]: "not json" }))).toBeUndefined();
    expect(
      droppedReference(carrying({ [REFERENCE_TYPE]: JSON.stringify({ path: "x" }) })),
    ).toBeUndefined();
    expect(droppedReference(carrying({ [REFERENCE_TYPE]: JSON.stringify(FILE) }))).toEqual(FILE);
  });
});

describe("what a reference handed to a chat says", () => {
  it("says it was typed and nothing was sent", () => {
    expect(handedSaid({ kind: "typed", text: "@src/main.rs" }, "steward 3")).toBe(
      "Typed @src/main.rs into steward 3. Nothing was sent.",
    );
  });

  it("says why it was copied instead, and what was copied", () => {
    expect(
      handedSaid(
        {
          kind: "copied",
          text: "@src/main.rs",
          why: "the chat is in the middle of a turn, so the reference is on the clipboard",
        },
        "steward 3",
      ),
    ).toBe(
      "The chat is in the middle of a turn, so the reference is on the clipboard: @src/main.rs",
    );
  });

  it("refuses a hit from another project without asking the core", async () => {
    const asked: string[] = [];
    mockIPC((cmd) => {
      asked.push(cmd);
    });

    const ran = await handReference("/p", 3, "steward 3", { ...FILE, plane: "/other" });

    expect(ran).toEqual({
      ok: false,
      refused: "src/main.rs is in another project, so it was not handed to steward 3.",
    });
    expect(asked).toEqual([]);
  });

  it("asks the core with the branch, the path and the lines, and says its answer", async () => {
    const asked: { cmd: string; args: unknown }[] = [];
    mockIPC((cmd, args) => {
      asked.push({ cmd, args });
      return { kind: "typed", text: "@src/main.rs#L4" };
    });

    const ran = await handReference("/p", 3, "steward 3", {
      ...FILE,
      lines: { first: 4, last: 4 },
    });

    expect(asked).toEqual([
      {
        cmd: "reference_into_chat",
        args: {
          plane: "/p",
          workspace: "alpha",
          repo: "svc",
          piece: "fix-it",
          path: "src/main.rs",
          lines: { first: 4, last: 4 },
          session: 3,
        },
      },
    ]);
    expect(ran).toEqual({
      ok: true,
      said: "Typed @src/main.rs#L4 into steward 3. Nothing was sent.",
    });
  });
});
