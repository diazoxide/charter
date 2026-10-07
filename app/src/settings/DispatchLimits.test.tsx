import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { DispatchLimits, DispatchLimitRow } from "../bindings";
import { DISPATCH, DispatchLimitsTable, dispatchGroup, workspaceDispatchGroup } from "./dispatch";
import type { LiveSetting } from "./groups";

/**
 * **Settings › Project › Dispatch** (#1439, #1440): the limits table (the project's row, a row
 * per workspace or persona override, Add an override), yours on this machine, the policy's
 * ceilings drawn locked, and the slot the dispatch grants fill. The same limits are edited on a
 * workspace's settings and on the persona view. Every rule is the core's: the window writes one
 * key of a settings file through `save_project_settings` and draws what the core answers.
 */

const PLANE = "/home/dev/plane";
const SHARED = "schema = 1\n";

const LIMITS: DispatchLimits["limits"] = [
  ["running-per-chat", "Running per chat", 6, false, 10000],
  ["live-per-lineage", "Live per lineage", 16, false, 10000],
  ["depth", "Depth", 3, false, 8],
  ["messages-per-minute", "Messages per minute", 10, false, 10000],
  ["may-dispatch", "May dispatch", null, true, 10000],
  ["may-run-at-once", "May run at once", null, true, 10000],
].map(([word, label, standard, persona_only, most]) => ({
  word: word as string,
  label: label as string,
  help: `What ${String(label).toLowerCase()} limits.`,
  default: standard as number | null,
  persona_only: persona_only as boolean,
  most: most as number,
  ceiling: null,
}));

const DEFAULTS = [6, 16, 3, 10, null, null];

function row(
  scope: string,
  name: string,
  values: (number | null)[] = [null, null, null, null, null, null],
  beneath: (number | null)[] = DEFAULTS,
): DispatchLimitRow {
  return { scope, name, values, beneath, ignored: [] };
}

function page(over: Partial<DispatchLimits> = {}): DispatchLimits {
  return {
    file: "purlis.toml",
    local_file: "purlis.local.toml",
    base: SHARED,
    local_base: null,
    limits: LIMITS,
    rows: [row("project", "")],
    mine: [row("project", "")],
    locked_by: null,
    refused: [],
    local_left_out: null,
    policy_refused: false,
    ...over,
  };
}

type Saved = { which: string; base: string | null; change: unknown };

/** The core: answers the page, the names an override may be for, and each save. */
function core(
  first: DispatchLimits,
  save: (asked: Saved) => { now?: DispatchLimits; refused?: string[] } = () => ({}),
) {
  let now = first;
  const saves: Saved[] = [];
  let reads = 0;
  mockIPC((cmd, args) => {
    if (cmd === "dispatch_limits") {
      reads += 1;
      return now;
    }
    if (cmd === "start_options")
      return { profiles: [], refused: [], personas: ["devops", "steward"] };
    if (cmd === "plane_sidebar") return { workspaces: [{ name: "alpha" }, { name: "beta" }] };
    if (cmd === "save_project_settings") {
      const asked = args as unknown as Saved;
      saves.push(asked);
      const said = save(asked);
      if (said.refused !== undefined) return { kind: "refused", reasons: said.refused };
      if (said.now !== undefined) now = said.now;
      return { kind: "saved", file: {} };
    }
    return undefined;
  });
  return { saves, reads: () => reads };
}

afterEach(() => {
  cleanup();
  clearMocks();
});

/** One setting of the Dispatch page, drawn as its row draws it. */
function Setting({ id }: { id: string }) {
  const setting = dispatchGroup(PLANE).settings.find((one) => one.id === id) as LiveSetting;
  const { control, undo } = setting.useControl();
  return (
    <>
      {control({ id: "d", labelledBy: "d-label" })}
      {/* The settings row draws the last change's Undo; here it is drawn bare. */}
      {undo && (
        <button type="button" onClick={undo}>
          Undo
        </button>
      )}
    </>
  );
}

const Page = () => <Setting id={`${DISPATCH}.limits`} />;

/** Types `text` into `box` and leaves it, which is when a limit is written. */
function typeInto(box: HTMLElement, text: string) {
  fireEvent.change(box, { target: { value: text } });
  fireEvent.blur(box);
}

const edit = (path: string[], value: unknown) => ({
  kind: "edits",
  edits: [{ path: path.map((key) => ({ key })), value }],
});

describe("Settings › Project › Dispatch", () => {
  it("is a page of the Project level, with the limits, the grants' slot and the locks", () => {
    const group = dispatchGroup(PLANE);
    expect(group.id).toBe("project.dispatch");
    expect(group.label).toBe("Dispatch");
    expect(group.settings.map((one) => one.id)).toEqual([
      "project.dispatch.limits",
      "project.dispatch.grants",
      "project.dispatch.locks",
    ]);
  });

  it("draws the project's row with what is in force where it sets nothing", async () => {
    core(page({ rows: [row("project", "", [4, null, null, null, null, null])] }));
    render(<Page />);

    const table = await screen.findByRole("table", { name: "Dispatch limits" });
    expect(
      within(table)
        .getAllByRole("columnheader")
        .map((one) => one.textContent),
    ).toEqual([
      "Level",
      "Running per chat",
      "Live per lineage",
      "Depth",
      "Messages per minute",
      "May dispatch",
      "May run at once",
      "",
    ]);
    expect(within(table).getByRole("rowheader", { name: "Project" })).toBeInTheDocument();
    expect(screen.getByLabelText("Running per chat for the project")).toHaveValue("4");
    // Not set here: the box is empty and shows the value beneath it.
    const depth = screen.getByLabelText("Depth for the project");
    expect(depth).toHaveValue("");
    expect(depth).toHaveAttribute("placeholder", "3");
    // A persona's limits are not the project's to set.
    expect(screen.queryByLabelText("May dispatch for the project")).not.toBeInTheDocument();
  });

  it("draws a row per workspace and persona override, a persona's with its two own limits", async () => {
    core(
      page({
        rows: [
          row("project", ""),
          row("workspace", "alpha", [12, null, null, null, null, null]),
          row("persona", "devops", [null, null, 1, null, null, 2]),
        ],
      }),
    );
    render(<Page />);

    await screen.findByRole("table", { name: "Dispatch limits" });
    expect(screen.getByRole("rowheader", { name: "Workspace alpha" })).toBeInTheDocument();
    expect(screen.getByRole("rowheader", { name: "Persona devops" })).toBeInTheDocument();
    expect(screen.getByLabelText("Running per chat for the workspace alpha")).toHaveValue("12");
    expect(screen.getByLabelText("Depth for the persona devops")).toHaveValue("1");
    expect(screen.getByLabelText("May run at once for the persona devops")).toHaveValue("2");
    // No cap until one is written.
    expect(screen.getByLabelText("May dispatch for the persona devops")).toHaveAttribute(
      "placeholder",
      "no cap",
    );
  });

  it("writes a changed limit into the project file and draws what the core then answers", async () => {
    const after = page({ rows: [row("project", "", [null, null, 2, null, null, null])] });
    const { saves, reads } = core(page(), () => ({ now: after }));
    render(<Page />);

    typeInto(await screen.findByLabelText("Depth for the project"), "2");

    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0]).toMatchObject({
      plane: PLANE,
      which: "shared",
      base: SHARED,
      change: edit(["dispatch", "depth"], { kind: "integer", value: 2 }),
    });
    // Read again, so what is shown is what is on disk.
    await waitFor(() => expect(reads()).toBe(2));
    expect(screen.getByLabelText("Depth for the project")).toHaveValue("2");
  });

  it("writes on Enter too, and nothing when the box is left as it was", async () => {
    const { saves } = core(page());
    render(<Page />);

    const depth = await screen.findByLabelText("Depth for the project");
    fireEvent.blur(depth);
    expect(saves).toHaveLength(0);
    await userEvent.type(depth, "5{Enter}");
    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0].change).toEqual(edit(["dispatch", "depth"], { kind: "integer", value: 5 }));
  });

  it("takes a limit out of the file when its box is emptied, so the level beneath is in force", async () => {
    const { saves } = core(page({ rows: [row("project", "", [4, null, null, null, null, null])] }));
    render(<Page />);

    typeInto(await screen.findByLabelText("Running per chat for the project"), "");

    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0].change).toEqual(edit(["dispatch", "running-per-chat"], null));
  });

  it("says the core's refusal of a depth of 9 and keeps showing what is on disk", async () => {
    const why =
      "dispatch.depth in purlis.toml is 9, and depth is never above 8, so it is not read and the level beneath it is in force";
    core(page(), () => ({ refused: [why] }));
    render(<Page />);

    typeInto(await screen.findByLabelText("Depth for the project"), "9");

    expect(await screen.findByText(why)).toBeInTheDocument();
    await waitFor(() => expect(screen.getByLabelText("Depth for the project")).toHaveValue(""));
  });

  it("sends what is not a number as it was typed, for the core to refuse", async () => {
    const { saves } = core(page(), () => ({ refused: ["dispatch.depth is not a whole number"] }));
    render(<Page />);

    typeInto(await screen.findByLabelText("Depth for the project"), "deep");

    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0].change).toEqual(edit(["dispatch", "depth"], { kind: "text", value: "deep" }));
    expect(await screen.findByText("dispatch.depth is not a whole number")).toBeInTheDocument();
  });

  it("adds an override for a workspace or a persona that has none, and writes its first limit", async () => {
    const { saves } = core(page({ rows: [row("project", ""), row("workspace", "alpha")] }));
    render(<Page />);

    const pick = await screen.findByRole("combobox", { name: "Override for" });
    // What has a row already is not offered again.
    await waitFor(() =>
      expect(
        within(pick)
          .getAllByRole("option")
          .map((one) => one.textContent),
      ).toEqual(["Workspace beta", "Persona devops", "Persona steward"]),
    );
    await userEvent.selectOptions(pick, "Persona devops");
    await userEvent.click(screen.getByRole("button", { name: "Add an override" }));

    // A row to fill in: nothing is written until a limit is.
    expect(screen.getByRole("rowheader", { name: "Persona devops" })).toBeInTheDocument();
    expect(saves).toHaveLength(0);
    typeInto(screen.getByLabelText("May run at once for the persona devops"), "1");
    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0]).toMatchObject({
      which: "shared",
      change: edit(["dispatch", "personas", "devops", "may-run-at-once"], {
        kind: "integer",
        value: 1,
      }),
    });
  });

  it("removes an override, table and all", async () => {
    const { saves } = core(
      page({
        rows: [row("project", ""), row("workspace", "alpha", [12, null, null, null, null, null])],
      }),
      () => ({ now: page() }),
    );
    render(<Page />);

    await userEvent.click(
      await screen.findByRole("button", { name: "Remove the override for the workspace alpha" }),
    );

    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0].change).toEqual(edit(["dispatch", "workspaces", "alpha"], null));
    await waitFor(() =>
      expect(screen.queryByRole("rowheader", { name: "Workspace alpha" })).not.toBeInTheDocument(),
    );
  });

  it("keeps your own limit on this machine, and says a higher one is ignored", async () => {
    const ignored =
      "Your own limit of 5 for depth is above the project's 3, so it is ignored: a limit on this machine can only lower one.";
    const { saves } = core(
      page({
        mine: [{ ...row("project", "", [null, null, 5, null, null, null]), ignored: [ignored] }],
      }),
    );
    render(<Page />);

    expect(
      await screen.findByRole("rowheader", { name: "Me on this machine" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Depth for me on this machine")).toHaveValue("5");
    expect(screen.getByText(ignored)).toBeInTheDocument();

    typeInto(screen.getByLabelText("Depth for me on this machine"), "2");
    await waitFor(() => expect(saves).toHaveLength(1));
    // This machine's file, which was not there: no base.
    expect(saves[0]).toMatchObject({
      which: "local",
      base: null,
      change: edit(["dispatch", "depth"], { kind: "integer", value: 2 }),
    });
  });

  it("still draws and applies your row while git would carry this machine's file, and says so", async () => {
    core(
      page({
        mine: [row("project", "", [null, null, 1, null, null, null])],
        local_left_out: "purlis.local.toml is tracked by git, so it is not read.",
      }),
    );
    render(<Page />);

    await screen.findByRole("table", { name: "Dispatch limits" });
    // A limit of yours can only lower one, so it holds whatever git says of the file.
    expect(screen.getByLabelText("Depth for me on this machine")).toHaveValue("1");
    expect(
      screen.getByText(
        "purlis.local.toml is tracked by git, so it is not read. Your dispatch limits in it still apply, because they can only lower one.",
      ),
    ).toBeInTheDocument();
  });

  it("offers Undo for the last limit changed, which puts back what the box held", async () => {
    const before = page({ rows: [row("project", "", [null, null, 4, null, null, null])] });
    const after = page({ rows: [row("project", "", [null, null, 2, null, null, null])] });
    let now = after;
    const { saves } = core(before, () => ({ now }));
    render(<Page />);

    // Nothing to undo until something is changed here.
    await screen.findByRole("table", { name: "Dispatch limits" });
    expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
    typeInto(screen.getByLabelText("Depth for the project"), "2");
    await waitFor(() => expect(saves).toHaveLength(1));

    now = before;
    await userEvent.click(await screen.findByRole("button", { name: "Undo" }));
    await waitFor(() => expect(saves).toHaveLength(2));
    expect(saves[1]).toMatchObject({
      which: "shared",
      change: edit(["dispatch", "depth"], { kind: "integer", value: 4 }),
    });
    // One level: the Undo is gone once it is used.
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument(),
    );
    expect(screen.getByLabelText("Depth for the project")).toHaveValue("4");
  });

  it("undoes a first value by taking the key out again", async () => {
    const { saves } = core(page(), () => ({
      now: page({ rows: [row("project", "", [null, null, 2, null, null, null])] }),
    }));
    render(<Page />);

    typeInto(await screen.findByLabelText("Depth for the project"), "2");
    await userEvent.click(await screen.findByRole("button", { name: "Undo" }));

    await waitFor(() => expect(saves).toHaveLength(2));
    expect(saves[1].change).toEqual(edit(["dispatch", "depth"], null));
  });

  it("undoes a removed override by writing back each limit it set", async () => {
    const { saves } = core(
      page({
        rows: [row("project", ""), row("persona", "devops", [null, null, 1, null, null, 2])],
      }),
      () => ({ now: page() }),
    );
    render(<Page />);

    await userEvent.click(
      await screen.findByRole("button", { name: "Remove the override for the persona devops" }),
    );
    await userEvent.click(await screen.findByRole("button", { name: "Undo" }));

    await waitFor(() => expect(saves).toHaveLength(2));
    expect(saves[1].change).toEqual({
      kind: "edits",
      edits: [
        {
          path: ["dispatch", "personas", "devops", "depth"].map((key) => ({ key })),
          value: { kind: "integer", value: 1 },
        },
        {
          path: ["dispatch", "personas", "devops", "may-run-at-once"].map((key) => ({ key })),
          value: { kind: "integer", value: 2 },
        },
      ],
    });
  });

  it("offers no Undo for a change the core refused", async () => {
    core(page(), () => ({ refused: ["dispatch.depth is 9, and depth is never above 8"] }));
    render(<Page />);

    typeInto(await screen.findByLabelText("Depth for the project"), "9");

    await screen.findByText("dispatch.depth is 9, and depth is never above 8");
    expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
  });

  it("draws a policy's ceiling as locked, with who set it, and no control", async () => {
    core(
      page({
        limits: LIMITS.map((one) => (one.word === "depth" ? { ...one, ceiling: 2 } : one)),
        locked_by: "Locked by policy, set by IT in /etc/purlis/policy.json.",
      }),
    );
    render(<Setting id={`${DISPATCH}.locks`} />);

    const locks = await screen.findByRole("list", { name: "Policy locks" });
    expect(
      within(locks)
        .getAllByRole("listitem")
        .map((one) => one.textContent),
    ).toEqual([
      "Depth is at most 2, whatever is set here. Locked by policy, set by IT in /etc/purlis/policy.json.",
    ]);
    expect(within(locks).queryByRole("textbox")).not.toBeInTheDocument();
    expect(within(locks).queryByRole("button")).not.toBeInTheDocument();
  });

  it("says a refused policy file is refused, once, and not that each limit is 0", async () => {
    const refused =
      "Locked by policy: this machine's policy file, /etc/purlis/policy.json, is refused (it is not JSON), so everything it could lock is locked until this machine's administrator fixes it.";
    core(
      page({
        limits: LIMITS.map((one) => ({ ...one, ceiling: 0 })),
        locked_by: refused,
        policy_refused: true,
      }),
    );
    render(<Setting id={`${DISPATCH}.locks`} />);

    const locks = await screen.findByRole("list", { name: "Policy locks" });
    expect(
      within(locks)
        .getAllByRole("listitem")
        .map((one) => one.textContent),
    ).toEqual([`Dispatch is off on this machine. ${refused}`]);
    expect(within(locks).queryByText(/is 0/)).not.toBeInTheDocument();
  });

  it("says a policy's 0 messages a minute stops messages, not dispatch", async () => {
    const by = "Locked by policy, set by IT in /etc/purlis/policy.json.";
    core(
      page({
        limits: LIMITS.map((one) =>
          one.word === "messages-per-minute" || one.word === "depth" ? { ...one, ceiling: 0 } : one,
        ),
        locked_by: by,
      }),
    );
    render(<Setting id={`${DISPATCH}.locks`} />);

    const locks = await screen.findByRole("list", { name: "Policy locks" });
    expect(
      within(locks)
        .getAllByRole("listitem")
        .map((one) => one.textContent),
    ).toEqual([
      `Depth is 0, so dispatch is off on this machine. ${by}`,
      `Messages per minute is 0, so no chat sends another a message on this machine. ${by}`,
    ]);
  });

  it("says no policy limits dispatch where none does", async () => {
    core(page());
    render(<Setting id={`${DISPATCH}.locks`} />);
    expect(
      await screen.findByText("No policy limits dispatch on this machine."),
    ).toBeInTheDocument();
  });

  it("lists the dispatch grants, each with Revoke", async () => {
    core(page());
    const grants = {
      grants: [
        {
          id: "you\u001fsteward\u001fdevops",
          asking: "steward",
          target: "devops",
          level: "you",
          by: null,
          at: null,
          chat: null,
          locked: null,
          waiting: false,
        },
      ],
      all_locked: null,
      locked_pairs: [{ asking: "qa", target: "devops" }],
      locked_by: "Locked by policy, set by IT in /etc/purlis/policy.json.",
      changed: null,
    };
    // After `core`, so this is the one the window asks.
    mockIPC((cmd) => (cmd === "dispatch_grants" ? grants : undefined));
    render(<Setting id={`${DISPATCH}.grants`} />);

    const list = await screen.findByRole("list", { name: "Dispatch grants" });
    expect(within(list).getByRole("listitem")).toHaveTextContent(
      "steward chats may dispatch to devops · Me on this machine · granted by you",
    );
    expect(
      screen.getByRole("button", { name: "Revoke steward dispatching to devops" }),
    ).toBeInTheDocument();
    // What policy locks is the next row's, not said twice.
    expect(screen.queryByRole("list", { name: "Locked by policy" })).not.toBeInTheDocument();
  });

  it("says there are no dispatch grants where there are none", async () => {
    core(page());
    render(<Setting id={`${DISPATCH}.grants`} />);
    expect(
      await screen.findByText("No persona's chats may dispatch to another persona yet."),
    ).toBeInTheDocument();
  });

  it("lists a locked pair and a lock on all dispatch with the policy's ceilings", async () => {
    const limits = page({
      limits: LIMITS.map((one) => (one.word === "depth" ? { ...one, ceiling: 2 } : one)),
      locked_by: "Locked by policy, set by IT in /etc/purlis/policy.json.",
    });
    const all =
      "Policy forbids one chat dispatching to another. Locked by policy, set by IT in /etc/purlis/policy.json.";
    mockIPC((cmd) => {
      if (cmd === "dispatch_limits") return limits;
      if (cmd === "dispatch_grants")
        return {
          grants: [],
          all_locked: all,
          locked_pairs: [{ asking: "steward", target: "devops" }],
          locked_by: "Locked by policy, set by IT in /etc/purlis/policy.json.",
          changed: null,
        };
      return undefined;
    });
    render(<Setting id={`${DISPATCH}.locks`} />);

    const locks = await screen.findByRole("list", { name: "Policy locks" });
    expect(
      within(locks)
        .getAllByRole("listitem")
        .map((one) => one.textContent),
    ).toEqual([
      "Depth is at most 2, whatever is set here. Locked by policy, set by IT in /etc/purlis/policy.json.",
      all,
      "steward chats may not dispatch to devops. Locked by policy, set by IT in /etc/purlis/policy.json.",
    ]);
    expect(within(locks).queryByRole("button")).not.toBeInTheDocument();
  });

  it("says what the core could not read, and offers no table", async () => {
    mockIPC((cmd) => {
      if (cmd === "dispatch_limits")
        throw "purlis.toml is not UTF-8 text, so purlis cannot show it";
      return undefined;
    });
    render(<Page />);
    expect(
      await screen.findByText("purlis.toml is not UTF-8 text, so purlis cannot show it"),
    ).toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
  });
});

describe("a workspace's settings", () => {
  function Workspace() {
    const group = workspaceDispatchGroup(PLANE, "alpha");
    expect(group.id).toBe("workspace.dispatch");
    const { control } = (group.settings[0] as LiveSetting).useControl();
    return <>{control({ id: "w", labelledBy: "w-label" })}</>;
  }

  it("edit this workspace's own limits, over the project's", async () => {
    const { saves } = core(
      page({
        rows: [
          row("project", "", [4, null, null, null, null, null]),
          row("workspace", "alpha", [null, 9, null, null, null, null], [4, 16, 3, 10, null, null]),
          row("workspace", "beta", [1, null, null, null, null, null]),
        ],
      }),
    );
    render(<Workspace />);

    const table = await screen.findByRole("table", { name: "Dispatch limits" });
    // This workspace's row and no other.
    expect(
      within(table)
        .getAllByRole("rowheader")
        .map((one) => one.textContent),
    ).toEqual(["This workspace"]);
    expect(screen.getByLabelText("Live per lineage for this workspace")).toHaveValue("9");
    // The project's 4 is in force until this workspace sets its own.
    expect(screen.getByLabelText("Running per chat for this workspace")).toHaveAttribute(
      "placeholder",
      "4",
    );
    // A persona's limits are not a workspace's.
    expect(screen.queryByLabelText("May dispatch for this workspace")).not.toBeInTheDocument();

    typeInto(screen.getByLabelText("Running per chat for this workspace"), "12");
    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0]).toMatchObject({
      which: "shared",
      change: edit(["dispatch", "workspaces", "alpha", "running-per-chat"], {
        kind: "integer",
        value: 12,
      }),
    });
  });

  it("add the override by typing its first limit, and remove it again", async () => {
    const withRow = page({
      rows: [row("project", ""), row("workspace", "alpha", [null, null, 2, null, null, null])],
    });
    const { saves } = core(
      page({ rows: [row("project", "", [null, null, 5, null, null, null])] }),
      () => ({
        now: withRow,
      }),
    );
    render(<Workspace />);

    // No override yet: the row is there to fill in, on the project's values.
    const depth = await screen.findByLabelText("Depth for this workspace");
    expect(depth).toHaveAttribute("placeholder", "5");
    expect(screen.queryByRole("button", { name: /Remove/ })).not.toBeInTheDocument();
    typeInto(depth, "2");
    await waitFor(() => expect(saves).toHaveLength(1));

    await userEvent.click(
      await screen.findByRole("button", { name: "Remove the override for this workspace" }),
    );
    await waitFor(() => expect(saves).toHaveLength(2));
    expect(saves[1].change).toEqual(edit(["dispatch", "workspaces", "alpha"], null));
  });
});

describe("the persona view", () => {
  it("edits this persona's own limits, its two own among them, in the project file", async () => {
    const { saves } = core(
      page({
        rows: [row("project", ""), row("persona", "devops", [null, null, 1, null, null, 2])],
      }),
    );
    render(<DispatchLimitsTable plane={PLANE} scope={{ kind: "persona", name: "devops" }} />);

    const table = await screen.findByRole("table", { name: "Dispatch limits" });
    expect(
      within(table)
        .getAllByRole("rowheader")
        .map((one) => one.textContent),
    ).toEqual(["This persona"]);
    expect(screen.getByLabelText("May run at once for this persona")).toHaveValue("2");
    // Kept in the project's file, never the persona's own, and it says so.
    expect(
      screen.getByText(
        /Kept in purlis\.toml, which your team sees, and never in the persona's own file/,
      ),
    ).toBeInTheDocument();

    typeInto(screen.getByLabelText("May dispatch for this persona"), "3");
    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0]).toMatchObject({
      which: "shared",
      change: edit(["dispatch", "personas", "devops", "may-dispatch"], {
        kind: "integer",
        value: 3,
      }),
    });
  });

  it("offers Undo under the table for the last limit changed there", async () => {
    const before = page({
      rows: [row("project", ""), row("persona", "devops", [null, null, 1, null, null, 2])],
    });
    const { saves } = core(before, () => ({
      now: page({
        rows: [row("project", ""), row("persona", "devops", [null, null, 1, null, null, 5])],
      }),
    }));
    render(<DispatchLimitsTable plane={PLANE} scope={{ kind: "persona", name: "devops" }} />);

    typeInto(await screen.findByLabelText("May run at once for this persona"), "5");
    await userEvent.click(await screen.findByRole("button", { name: "Undo" }));

    await waitFor(() => expect(saves).toHaveLength(2));
    expect(saves[1].change).toEqual(
      edit(["dispatch", "personas", "devops", "may-run-at-once"], { kind: "integer", value: 2 }),
    );
  });

  it("draws nothing where the core says nothing of the limits", async () => {
    mockIPC(() => undefined);
    const { container } = render(
      <DispatchLimitsTable plane={PLANE} scope={{ kind: "persona", name: "devops" }} />,
    );
    await waitFor(() =>
      expect(screen.queryByText("Reading the dispatch limits…")).not.toBeInTheDocument(),
    );
    expect(container.querySelector("table")).toBeNull();
  });
});
