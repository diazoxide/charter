import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type {
  AwayRefusal,
  ChatBlocked,
  DoctorReport,
  DoctorRow,
  FinishedTask,
  InboxUpdate,
  OpenChat,
  UpdateNoted,
} from "./bindings";
import {
  KEPT_FOR_MS,
  awayUpdates,
  doctorUpdates,
  listed,
  resumeUpdates,
  sandboxUpdates,
  smartCloseUpdates,
  taskUpdates,
  useInboxUpdates,
} from "./inboxUpdates";

/**
 * **The Inbox's updates** (#1693): what each source says becomes one update, kept a day by the
 * core, and what Mark all read and Dismiss all do. The sources are plain inputs; the core is a
 * mock that keeps what it is told, as the real store does.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const NOW = Date.parse("2026-10-11T12:00:00Z");
const PLANE = "/home/dev/plane";
const nameOf = (session: number) => `steward ${session}`;

const finished = (id: string, how: string, ended: string | null): FinishedTask =>
  ({
    id,
    asker: 3,
    name: `task ${id}`,
    chat: 9,
    persona: null,
    how,
    outcome: how,
    folds: false,
    report: "",
    changed: null,
    ended,
    place: "",
    branch: null,
    reopens: false,
    not_reopened: null,
    did_not_start: false,
    attempts: 1,
    waits: null,
  }) as unknown as FinishedTask;

describe("each source, said as updates", () => {
  it("makes a finished task one update, in the count its row is in, at the time it ended", () => {
    const ended = new Date(NOW - 60_000).toISOString();
    const said = taskUpdates(
      [finished("a", "done", ended), finished("b", "failed", ended), finished("c", "done", null)],
      nameOf,
      NOW,
    );
    expect(said.map((one) => [one.key, one.kind, one.session, one.chain])).toEqual([
      ["task:a", "task-done", 3, ["steward 3"]],
      ["task:b", "task-failed", 3, ["steward 3"]],
    ]);
    expect(said[0].at).toBe(Math.floor((NOW - 60_000) / 1000));
    expect(said[0].says).toMatch(/^task a: /);
  });

  it("leaves out a task that ended a day ago or more", () => {
    const old = new Date(NOW - KEPT_FOR_MS).toISOString();
    expect(taskUpdates([finished("a", "done", old)], nameOf, NOW)).toEqual([]);
  });

  it("makes each finding of the doctor an update, and no row it does not check", () => {
    const row = (name: string, status: DoctorRow["status"], checked = true): DoctorRow => ({
      name,
      status,
      detail: `${name} detail`,
      hint: "",
      checked,
      settings: null,
      fix: null,
    });
    const report = {
      rows: [row("git", "fail"), row("pin", "warn"), row("later", "warn", false), row("ok", "ok")],
      app_rows: [],
    } as unknown as DoctorReport;
    expect(doctorUpdates(report, NOW).map((one) => [one.key, one.says])).toEqual([
      ["doctor:git:fail", "git: git detail"],
      ["doctor:pin:warn", "pin: pin detail"],
    ]);
    expect(doctorUpdates(undefined, NOW)).toEqual([]);
  });

  it("makes a chat that came back an update, resumed or fresh, and a chat that simply started none", () => {
    const chat = (session: number, resumed: string | null, fresh: string | null) =>
      ({ session, name: `chat ${session}`, resumed, fresh }) as unknown as OpenChat;
    const said = resumeUpdates(
      [chat(1, "conv-1", null), chat(2, null, "its conversation was gone"), chat(3, null, null)],
      NOW,
    );
    expect(said.map((one) => [one.session, one.says])).toEqual([
      [1, "Resumed its conversation"],
      [2, "Came back as a new chat: its conversation was gone"],
    ]);
  });

  it("makes a block that is no host ask a sandbox update, and leaves a host ask to the asks", () => {
    const block = (offer: string, target: string | null): ChatBlocked =>
      ({
        plane: PLANE,
        session: 4,
        operation: offer === "host" ? "connect" : "write",
        kind: offer === "host" ? "host" : "folder",
        ours: false,
        harness: null,
        said: `blocked ${offer}`,
        offer,
        target,
        route: null,
        levels: [],
      }) as unknown as ChatBlocked;
    const said = sandboxUpdates(
      { 4: [block("host", "api.example.com"), block("write", "/srv")] },
      nameOf,
      NOW,
    );
    expect(said.map((one) => [one.kind, one.says, one.chain])).toEqual([
      ["sandbox", "blocked write", ["steward 4"]],
    ]);
  });

  it("makes a dispatch refused while nobody was there an update at its last refusal", () => {
    const refused = {
      asking: "steward",
      target: "devops",
      workspace: null,
      latest: 1_800_000_000,
      times: 2,
      allows: "",
      nowhere: null,
      shown: "d",
    } satisfies AwayRefusal;
    expect(awayUpdates([refused])).toEqual([
      {
        key: "away:steward#devops#",
        kind: "refused-away",
        at: 1_800_000_000,
        session: null,
        chain: [],
        says: "steward wanted devops while you were away",
      },
    ]);
  });

  it("makes a Smart close that stopped an update about its chat", () => {
    const said = smartCloseUpdates({ 6: "Smart close stopped — it ended" }, nameOf, NOW);
    expect(said.map((one) => [one.kind, one.session, one.says])).toEqual([
      ["smart-close", 6, "Smart close stopped — it ended"],
    ]);
  });
});

describe("the list", () => {
  const kept = (key: string, at: number, kind: InboxUpdate["kind"] = "task-done"): InboxUpdate => ({
    key,
    kind,
    at,
    session: null,
    chain: [],
    says: key,
    read: false,
  });

  it("is newest first, and drops an answered-at-its-source update its source no longer lists", () => {
    const list = listed(
      [
        kept("old", 1),
        kept("away:gone", 5, "refused-away"),
        kept("away:here", 3, "refused-away"),
        kept("new", 4),
      ],
      [{ ...kept("away:here", 3, "refused-away") }],
    );
    expect(list.map((one) => one.key)).toEqual(["new", "away:here", "old"]);
  });

  it("lists an update its source still lists though the core keeps it no longer", () => {
    // A dispatch refused two days ago is past the day the core keeps, and still answered here.
    const list = listed([kept("new", 4)], [kept("away:old", 1, "refused-away")]);
    expect(list.map((one) => [one.key, one.read])).toEqual([
      ["new", false],
      ["away:old", false],
    ]);
  });
});

/**
 * A core that keeps updates as `note_inbox_updates` and `settle_inbox_updates` do, and counts
 * what it is asked.
 */
function store(kept: InboxUpdate[] = []) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  const shown = () =>
    kept.filter((one) => !dismissed.has(one.key)).sort((one, other) => other.at - one.at);
  const dismissed = new Set<string>();
  mockIPC((cmd, args) => {
    const given = args as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "inbox_updates") return shown();
    if (cmd === "note_inbox_updates") {
      for (const one of given.noted as UpdateNoted[])
        if (!kept.some((was) => was.key === one.key)) kept.push({ ...one, read: false });
      return shown();
    }
    if (cmd === "settle_inbox_updates") {
      const keys = given.keys as string[] | null;
      for (const one of kept) {
        if (keys !== null && !keys.includes(one.key)) continue;
        one.read = true;
        if (given.how === "dismissed") dismissed.add(one.key);
      }
      return shown();
    }
    return null;
  });
  return asked;
}

const noted = (key: string, at: number): UpdateNoted => ({
  key,
  kind: "task-done",
  at,
  session: 3,
  chain: ["steward 3"],
  says: key,
});

describe("noting, keeping and settling (#1693)", () => {
  it("notes what its sources say once, and lists what the core keeps, newest first", async () => {
    const asked = store([{ ...noted("kept:before", 1), read: true }]);
    const derived = [noted("task:a", 10), noted("task:b", 20)];
    const { result, rerender } = renderHook(({ now }) => useInboxUpdates(PLANE, now), {
      initialProps: { now: derived },
    });
    await waitFor(() =>
      expect(result.current.updates?.map((one) => one.key)).toEqual([
        "task:b",
        "task:a",
        "kept:before",
      ]),
    );
    // The same sources said again write nothing more.
    rerender({ now: [...derived] });
    expect(asked.filter((one) => one.cmd === "note_inbox_updates")).toHaveLength(1);
  });

  it("marks every update read, and dismisses every one, on the core", async () => {
    const asked = store();
    const { result } = renderHook(() => useInboxUpdates(PLANE, DERIVED));
    await waitFor(() => expect(result.current.updates).toHaveLength(1));
    act(() => result.current.settle(null, "read"));
    await waitFor(() => expect(result.current.updates?.[0].read).toBe(true));
    act(() => result.current.settle(null, "dismissed"));
    await waitFor(() => expect(result.current.updates).toEqual([]));
    expect(
      asked
        .filter((one) => one.cmd === "settle_inbox_updates")
        .map((one) => [one.args.keys, one.args.how]),
    ).toEqual([
      [null, "read"],
      [null, "dismissed"],
    ]);
  });

  it("keeps the list in this window, and says why, where the machine keeps none", async () => {
    mockIPC((cmd) => {
      if (cmd.endsWith("inbox_updates")) throw "purlis has nowhere to keep the Inbox's updates";
      return null;
    });
    const { result } = renderHook(() => useInboxUpdates(PLANE, DERIVED));
    await waitFor(() => expect(result.current.trouble).toMatch(/nowhere to keep/));
    expect(result.current.updates?.map((one) => one.key)).toEqual(["task:a"]);
    act(() => result.current.settle(["task:a"], "dismissed"));
    await waitFor(() => expect(result.current.updates).toEqual([]));
  });
});

const DERIVED = [noted("task:a", 10)];
