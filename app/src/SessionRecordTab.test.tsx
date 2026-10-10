import { useCallback } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SessionRecordTab } from "./SessionRecordTab";
import { PieceFilesTab } from "./editor/PieceFiles";
import { useJumpAsks, type Jump, type Pending } from "./fileJump";
import { forgetYourEditor, setYourEditor } from "./yourEditor";
import type { SessionRecordView } from "./bindings";

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";
const PATH = "workspaces/ide/sessions/2026-10-09-a.md";

describe("a session record's tab", () => {
  it("says why its record could not be read, and Read again asks again (NO-8, #1296)", async () => {
    const asked: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd !== "session_record") return null;
      asked.push(args);
      if (asked.length === 1) throw "the record could not be read: permission denied";
      return null;
    });
    render(<SessionRecordTab plane={PLANE} path={PATH} />);

    expect(await screen.findByText(/permission denied/)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Read again" }));

    // The second read answers that nothing is there, which is not an error.
    expect(await screen.findByText("This session record is not here any more")).toBeTruthy();
    expect(asked).toEqual([
      { plane: PLANE, path: PATH },
      { plane: PLANE, path: PATH },
    ]);
  });
});

/** A record of `place` whose text is `body`, as `session_record` answers it. */
function record(place: string, body: string): SessionRecordView {
  return {
    row: {
      path: PATH,
      title: "Fixing the reader",
      when: "2026-10-09 14:02",
      persona: null,
      harness: "claude",
      resumable: false,
    },
    place,
    body,
    persona_hosts: [],
    resume_holds: false,
    persona_hosts_locked: null,
    persona_hosts_wait: false,
    dispatches: [],
  };
}

/** The core, for a record of `place` saying `body` in a workspace whose repos are `repos`. */
function core(place: string, body: string, repos: string[], files: Record<string, string> = {}) {
  const asked: string[] = [];
  mockIPC(
    (cmd, args) => {
      const a = args as Record<string, unknown>;
      if (cmd === "session_record") return record(place, body);
      if (cmd === "workspace_repos") {
        asked.push(`${cmd}:${String(a.workspace)}`);
        return {
          workspace: a.workspace,
          repos: repos.map((name) => ({ name })),
          cache_refused: null,
        };
      }
      if (cmd === "open_in_your_editor") {
        asked.push(`${cmd}:${a.workspace}/${a.repo}/${a.piece}:${a.path}:${a.line}:${a.editor}`);
        return null;
      }
      if (cmd === "branch_tree") return { entries: [], more: 0 };
      if (cmd === "branch_status") return { changes: [], folders: [], more: 0, base: "main" };
      if (cmd === "files_watch") return null;
      if (cmd === "piece_file") {
        asked.push(`${cmd}:${a.workspace}/${a.repo}/${a.piece}:${a.path}`);
        const text = files[String(a.path)];
        if (text === undefined) throw `${String(a.path)} is not a file this folder offers`;
        return { kind: "text", text };
      }
      throw new Error(`unexpected ${cmd}`);
    },
    { shouldMockEvents: true },
  );
  return asked;
}

/** Every jump the window hears while it is drawn. */
function Window({ heard }: { heard: Jump[] }) {
  const hear = useCallback(
    (jump: Pending) => {
      heard.push({ plane: jump.plane, place: jump.place, path: jump.path, line: jump.line });
    },
    [heard],
  );
  useJumpAsks(hear);
  return null;
}

const SVC = { workspace: "alpha", repo: "svc", piece: null };

describe("a file and line a session record names (#984, #1043)", () => {
  afterEach(() => forgetYourEditor());

  it("opens in the light editor at that line, in the repo the path names", async () => {
    core("alpha", "Fixed the read in `svc/src/lib.rs:3`, then ran `npm test`.", ["svc", "tool"]);
    const heard: Jump[] = [];
    render(
      <>
        <Window heard={heard} />
        <SessionRecordTab plane={PLANE} path={PATH} />
      </>,
    );
    const article = await screen.findByTestId("session-record");

    await userEvent.click(
      await within(article).findByRole("button", { name: "Open svc/src/lib.rs:3" }),
    );

    expect(heard).toEqual([{ plane: PLANE, place: SVC, path: "src/lib.rs", line: 3 }]);
    // Code that names no file stays code, with nothing drawn as a link.
    expect(within(article).getByText("npm test").closest("button")).toBeNull();
  });

  it("stays text when it names no repo and the workspace has several", async () => {
    const asked = core("alpha", "See `src/lib.rs:3`.", ["svc", "tool"]);
    render(<SessionRecordTab plane={PLANE} path={PATH} />);
    const article = await screen.findByTestId("session-record");

    await waitFor(() => expect(asked).toContain("workspace_repos:alpha"));
    expect(within(article).getByText("src/lib.rs:3").closest("button")).toBeNull();
    expect(screen.queryByRole("region", { name: "Files this record names" })).toBeNull();
  });

  it("offers none in a plane-root record, and does not ask for repos", async () => {
    const asked = core("plane root", "See `svc/src/lib.rs:3`.", ["svc"]);
    render(<SessionRecordTab plane={PLANE} path={PATH} />);
    const article = await screen.findByTestId("session-record");

    expect(within(article).getByText("svc/src/lib.rs:3").closest("button")).toBeNull();
    expect(asked).toEqual([]);
  });

  it("lands on the file tab's refusal for a path the folder does not offer", async () => {
    const asked = core("alpha", "Removed `gone.rs:4`.", ["svc"]);
    render(
      <>
        <SessionRecordTab plane={PLANE} path={PATH} />
        <PieceFilesTab plane={PLANE} cut={SVC} onOpenView={() => undefined} />
      </>,
    );

    await userEvent.click(await screen.findByRole("button", { name: "Open gone.rs:4" }));

    expect(await screen.findByText(/gone\.rs is not a file this folder offers/)).toBeVisible();
    expect(asked).toContain("piece_file:alpha/svc/null:gone.rs");
  });

  it("opens in your editor at that line too, once per file and line", async () => {
    setYourEditor("zed");
    const asked = core(
      "alpha",
      "`svc/src/lib.rs:3` and again `svc/src/lib.rs:3`, and `tool/README.md`.",
      ["svc", "tool"],
    );
    render(<SessionRecordTab plane={PLANE} path={PATH} />);
    const named = await screen.findByRole("region", { name: "Files this record names" });

    const rows = within(named).getAllByRole("listitem");
    expect(rows.map((row) => row.querySelector("code")?.textContent)).toEqual([
      "svc/src/lib.rs:3",
      "tool/README.md",
    ]);
    await userEvent.click(
      within(rows[0]).getByRole("button", { name: "Open in your editor at line 3" }),
    );

    await waitFor(() =>
      expect(asked).toContain("open_in_your_editor:alpha/svc/null:src/lib.rs:3:zed"),
    );
  });
});
