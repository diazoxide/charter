import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { DispatchGrant, DispatchGrants, DispatchStanding } from "../bindings";
import { DispatchGrantsList, dispatchSourceSaid, dispatchWorkspaceSaid } from "./DispatchGrants";
import { grantedGroup } from "./GrantedList";
import type { LiveSetting } from "./groups";
import { forgetGroups, settingsPlace, useShownGroup } from "./links";

/**
 * **The one table of dispatch grants in Settings** (#1504): a row group per persona, each pair
 * with every grant that covers it and where it comes from, "any persona", the pairs said never
 * to, what a removed persona left, and what an administrator's policy locks. Every press asks
 * first and says what it does.
 */

const PLANE = "/home/dev/plane";
const FILE = "purlis.toml";

const grant = (over: Partial<DispatchGrant>): DispatchGrant => ({
  id: "",
  asking: "steward",
  target: "devops",
  level: "you",
  by: null,
  at: null,
  chat: null,
  locked: null,
  waiting: false,
  declined: false,
  workspace: null,
  nowhere: null,
  ...over,
});

const CHAT = grant({
  id: "chat\u001fc1\u001fsteward\u001fqa",
  target: "qa",
  level: "chat",
  chat: "steward 3",
});
const MINE = grant({ id: "you\u001fsteward\u001fdevops" });
const OURS = grant({ id: "project\u001fsteward\u001fdevops", level: "project", by: "Dana" });
const THEIRS = grant({
  id: "project\u001fqa\u001fdevops",
  asking: "qa",
  level: "project",
  by: "Dana",
  waiting: true,
});

const state = (over: Partial<DispatchGrants> = {}): DispatchGrants => ({
  grants: [],
  all_locked: null,
  locked_pairs: [],
  locked_by: null,
  ...over,
});

/** What a grant of "any persona" that holds in any workspace says of its workspace. */
const ANYWHERE = { workspace: null, nowhere: null, id: null } as const;

const stands = (over: Partial<DispatchStanding> = {}): DispatchStanding => ({
  nevers: [],
  any: [],
  nevers_unread: null,
  project_unsettled: false,
  personas: ["devops", "qa", "steward"],
  kept_blocked: [],
  dormant: [],
  returned: [],
  wants: [],
  back: [],
  workspaces: [],
  ...over,
});

interface Asked {
  cmd: string;
  args: Record<string, unknown>;
}

/** A core that answers the two reads from what `now` says, and remembers what it was sent. */
function core(now: { grants?: DispatchGrant[]; standing?: Partial<DispatchStanding> }) {
  const asked: Asked[] = [];
  const held = { grants: now.grants ?? [], standing: stands(now.standing) };
  const writes: Record<string, (args: Record<string, unknown>) => void> = {};
  mockIPC((cmd, args) => {
    const sent = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: sent });
    if (cmd === "dispatch_grants") return state({ grants: held.grants });
    if (cmd === "dispatch_standing") return held.standing;
    const write = writes[cmd];
    if (write === undefined) return null;
    write(sent);
    return cmd === "revoke_dispatch_grant" ? state({ grants: held.grants }) : held.standing;
  });
  return {
    asked,
    held,
    /** What `cmd` does to what the core holds; one that throws is the core's refusal. */
    on(cmd: string, write: (args: Record<string, unknown>) => void) {
      writes[cmd] = write;
    },
    sent: (cmd: string) => asked.filter((one) => one.cmd === cmd).map((one) => one.args),
    /** Every command sent that is not one of the two reads. */
    wrote: () => asked.filter((one) => !["dispatch_grants", "dispatch_standing"].includes(one.cmd)),
  };
}

afterEach(() => {
  cleanup();
  clearMocks();
  forgetGroups();
});

const Table = () => (
  <DispatchGrantsList plane={PLANE} file={FILE} ids={{ id: "d", labelledBy: "d-label" }} />
);

const table = () => screen.findByRole("table", { name: /Who may dispatch to whom/ });

/** The row a button is in. */
const rowOf = (button: HTMLElement) => button.closest("tr") as HTMLElement;

/** Presses the button named `name`, and answers the question it asks with `yes`. */
async function press(name: string | RegExp, yes: string) {
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name }));
  const question = screen.getByRole("group", { name: "Confirm" });
  const said = question.textContent ?? "";
  await user.click(within(question).getByRole("button", { name: yes }));
  return said;
}

describe("the table of who may dispatch to whom", () => {
  it("says under a persona's name what its definition wants, and that it grants nothing", async () => {
    // #1502's `wants`, at #1504's marked place: told by the core, drawn as a note and never
    // as a row, since nothing is in force by it.
    core({
      grants: [],
      standing: { wants: [{ persona: "steward", wants: ["devops", "qa"] }] },
    });
    render(<Table />);

    const whole = await table();
    const heading = (name: string) =>
      within(whole)
        .getAllByRole("rowheader")
        .find(
          (one) => one.getAttribute("scope") === "rowgroup" && one.textContent.startsWith(name),
        );
    expect(heading("steward")).toHaveTextContent(
      "Its definition says it wants to dispatch to devops, qa. That grants nothing: it only offers those personas as boxes when one of its chats first asks you.",
    );
    expect(heading("qa")).not.toHaveTextContent("wants to dispatch");
    // No row, no button and no grant comes of it.
    expect(
      within(whole).queryByRole("button", { name: /Revoke|Accept|Allow for steward/ }),
    ).toBeNull();
    expect(whole).toHaveTextContent(
      "No grant names a persona. Its first dispatch to one asks you.",
    );
  });

  it("has a row group for every persona, and under it each pair with every grant that covers it", async () => {
    core({ grants: [CHAT, MINE, OURS, THEIRS] });
    render(<Table />);

    const whole = await table();
    expect(
      within(whole)
        .getAllByRole("columnheader")
        .map((one) => one.textContent),
    ).toEqual(["May dispatch to", "Where it comes from", "In which workspace", "Action"]);
    // A row group per persona, the ones nothing is granted for included.
    const groups = within(whole)
      .getAllByRole("rowheader")
      .filter((one) => one.getAttribute("scope") === "rowgroup")
      .map((one) => one.textContent);
    expect(groups).toEqual(["devops", "qa", "steward"]);

    // steward to devops is covered twice: both sources are rows under the one target.
    const mine = rowOf(
      screen.getByRole("button", { name: "Revoke: my grant for steward to devops" }),
    );
    expect(within(mine).getByRole("rowheader")).toHaveTextContent("devops");
    expect(within(mine).getByRole("rowheader")).toHaveAttribute("rowspan", "2");
    expect(mine).toHaveTextContent("Me on this machine");
    expect(mine).toHaveTextContent("Any workspace");
    const ours = rowOf(
      screen.getByRole("button", {
        name: "Remove for everyone: the project's grant for steward to devops",
      }),
    );
    expect(ours).toHaveTextContent("The project, committed by Dana");
    expect(
      within(ours).getByRole("button", {
        name: "Not on my machine: stop following the project's grant for steward to devops",
      }),
    ).toBeInTheDocument();
    expect(within(ours).queryByRole("rowheader")).toBeNull();

    // A grant for one chat says which chat.
    const chat = rowOf(
      screen.getByRole("button", { name: "Revoke: this chat's grant for steward to qa" }),
    );
    expect(chat).toHaveTextContent("This chat only: steward 3");

    // A teammate's grant nobody here accepted is drawn waiting, with Accept.
    const theirs = rowOf(
      screen.getByRole("button", {
        name: "Accept: the project's grant for qa to devops, on this machine",
      }),
    );
    expect(theirs).toHaveTextContent(
      "Waiting for you: it is the project's, and it allows nothing on this machine until you accept it.",
    );
  });

  it("says where a grant comes from, with what is not known left out", () => {
    expect(dispatchSourceSaid(MINE)).toBe("Me on this machine");
    expect(dispatchSourceSaid({ ...MINE, chat: "steward 1" })).toBe(
      "Me on this machine, from steward 1",
    );
    expect(dispatchSourceSaid(CHAT)).toBe("This chat only: steward 3");
    expect(dispatchSourceSaid(OURS)).toBe("The project, committed by Dana");
    expect(dispatchSourceSaid({ ...OURS, by: null })).toBe("The project, not committed yet");
  });

  it("says when nothing is granted and the project has no persona to list", async () => {
    core({ standing: { personas: [] } });
    render(<Table />);
    expect(
      await screen.findByText(/No persona's chats may dispatch to another persona yet\./),
    ).toBeInTheDocument();
    expect(screen.queryByRole("table")).toBeNull();
  });
});

describe("taking a grant back", () => {
  it("asks first, says it stops new dispatches only, and Revoke goes through the core", async () => {
    const fake = core({ grants: [MINE, CHAT] });
    fake.on("revoke_dispatch_grant", ({ id }) => {
      fake.held.grants = fake.held.grants.filter((one) => one.id !== id);
    });
    render(<Table />);

    const said = await press("Revoke: my grant for steward to devops", "Revoke");

    expect(said).toContain(
      "The next dispatch from steward to devops asks you again. Tasks already running are left as they are.",
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: "Revoke: my grant for steward to devops" }),
      ).toBeNull(),
    );
    expect(fake.sent("revoke_dispatch_grant")).toEqual([{ plane: PLANE, id: MINE.id }]);
    expect(
      screen.getByText(
        "Revoked. The next dispatch from steward to devops asks you. Tasks already running are left as they are.",
      ),
    ).toBeInTheDocument();
    // The other grant was not touched.
    expect(
      screen.getByRole("button", { name: "Revoke: this chat's grant for steward to qa" }),
    ).toBeInTheDocument();
  });

  it("sends nothing until the person confirms, and Cancel sends nothing at all", async () => {
    const fake = core({ grants: [MINE] });
    render(<Table />);
    const user = userEvent.setup();

    await user.click(
      await screen.findByRole("button", { name: "Revoke: my grant for steward to devops" }),
    );
    expect(fake.wrote()).toEqual([]);
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    expect(fake.wrote()).toEqual([]);
    expect(screen.queryByRole("group", { name: "Confirm" })).toBeNull();
    expect(
      screen.getByRole("button", { name: "Revoke: my grant for steward to devops" }),
    ).toHaveFocus();
  });

  it("says a project grant's removal changes the committed file before it does", async () => {
    const fake = core({ grants: [OURS] });
    fake.on("revoke_dispatch_grant", () => {
      fake.held.grants = [];
    });
    render(<Table />);
    const user = userEvent.setup();

    await user.click(
      await screen.findByRole("button", {
        name: "Remove for everyone: the project's grant for steward to devops",
      }),
    );
    const question = screen.getByRole("group", { name: "Confirm" });
    expect(question).toHaveTextContent(
      "Remove this grant for everyone? This changes purlis.toml, the project's committed file: your teammates lose the grant when they pull it. Tasks already running are left as they are.",
    );
    expect(fake.wrote()).toEqual([]);

    await user.click(within(question).getByRole("button", { name: "Remove for everyone" }));

    await waitFor(() =>
      expect(fake.sent("revoke_dispatch_grant")).toEqual([{ plane: PLANE, id: OURS.id }]),
    );
    expect(await screen.findByText(/^Removed from purlis\.toml\./)).toBeInTheDocument();
  });

  it("stops following a project grant on this machine without touching the file", async () => {
    const fake = core({ grants: [OURS] });
    fake.on("decline_project_dispatch", () => {
      fake.held.grants = [{ ...OURS, waiting: true, declined: true }];
    });
    render(<Table />);

    const said = await press(
      "Not on my machine: stop following the project's grant for steward to devops",
      "Not on my machine",
    );

    expect(said).toContain("purlis.toml is not changed, so your teammates keep it.");
    expect(said).toContain("Tasks already running are left as they are.");
    await waitFor(() =>
      expect(fake.sent("decline_project_dispatch")).toEqual([
        { plane: PLANE, asking: "steward", target: "devops" },
      ]),
    );
    expect(fake.sent("revoke_dispatch_grant")).toEqual([]);
    // It is still the project's, drawn as not followed here, and Accept brings it back.
    const row = rowOf(
      await screen.findByRole("button", {
        name: "Accept: the project's grant for steward to devops, on this machine",
      }),
    );
    expect(row).toHaveTextContent(
      "Not followed on this machine: you said so. It allows nothing here.",
    );
  });

  it("accepts a teammate's grant that was waiting", async () => {
    const fake = core({ grants: [THEIRS] });
    fake.on("accept_project_dispatch", () => {
      fake.held.grants = [{ ...THEIRS, waiting: false }];
    });
    render(<Table />);

    const said = await press(
      "Accept: the project's grant for qa to devops, on this machine",
      "Accept",
    );

    expect(said).toContain("qa chats will dispatch to devops here without asking you.");
    await waitFor(() =>
      expect(fake.sent("accept_project_dispatch")).toEqual([
        { plane: PLANE, asking: "qa", target: "devops" },
      ]),
    );
    await waitFor(() => expect(screen.queryByText(/Waiting for you/)).toBeNull());
  });

  it("reads the table again when the core refuses, so no row is left out of date", async () => {
    const fake = core({ grants: [MINE] });
    fake.on("revoke_dispatch_grant", () => {
      // The grant went while the table was open: the core refuses, and holds none now.
      fake.held.grants = [];
      throw "purlis did not revoke it: that grant is no longer there.";
    });
    render(<Table />);

    await press("Revoke: my grant for steward to devops", "Revoke");

    expect(
      await screen.findByText("purlis did not revoke it: that grant is no longer there."),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: "Revoke: my grant for steward to devops" }),
      ).toBeNull(),
    );
    expect(fake.sent("dispatch_grants").length).toBeGreaterThan(1);
    // Nothing is said to have been done.
    expect(screen.queryByText(/^Revoked\./)).toBeNull();
  });
});

describe("any persona", () => {
  it("is set per persona, for me, and the question says it covers personas added later", async () => {
    const fake = core({});
    fake.on("allow_dispatch_to_any", ({ asking, level }) => {
      fake.held.standing = stands({
        any: [
          {
            asking: asking as string,
            level: level as "you",
            waiting: false,
            declined: false,
            ...ANYWHERE,
          },
        ],
      });
    });
    render(<Table />);
    await table();
    // Each persona has its own, for me and for the project.
    for (const persona of ["devops", "qa", "steward"])
      for (const whom of ["Allow for me", "Allow for everyone"])
        expect(
          screen.getByRole("button", {
            name: `${whom}: ${persona} may dispatch to any persona`,
          }),
        ).toBeInTheDocument();

    const said = await press("Allow for me: steward may dispatch to any persona", "Allow for me");

    expect(said).toBe(
      "Let steward dispatch to any persona, for you on this machine? steward chats will dispatch to every persona of this project without asking you, and to any persona added later.Allow for meCancel",
    );
    await waitFor(() =>
      expect(fake.sent("allow_dispatch_to_any")).toEqual([
        { plane: PLANE, asking: "steward", level: "you" },
      ]),
    );
    const row = rowOf(
      await screen.findByRole("button", {
        name: "Clear: any persona for steward, for me on this machine",
      }),
    );
    expect(row).toHaveTextContent("Me on this machine: allowed");
    expect(within(row).getByRole("rowheader")).toHaveTextContent("Any persona");
  });

  it("is set for the project, saying it changes the committed file, and is cleared here", async () => {
    const fake = core({
      standing: {
        any: [{ asking: "qa", level: "you", waiting: false, declined: false, ...ANYWHERE }],
      },
    });
    fake.on("allow_dispatch_to_any", () => {});
    fake.on("revoke_dispatch_to_any", () => {
      fake.held.standing = stands();
    });
    render(<Table />);

    const said = await press(
      "Allow for everyone: steward may dispatch to any persona",
      "Allow for everyone",
    );
    expect(said).toContain("This changes purlis.toml, the project's committed file.");
    expect(said).toContain("and to any persona added later.");
    await waitFor(() =>
      expect(fake.sent("allow_dispatch_to_any")).toEqual([
        { plane: PLANE, asking: "steward", level: "project" },
      ]),
    );

    const cleared = await press("Clear: any persona for qa, for me on this machine", "Clear");
    expect(cleared).toContain("Grants that name a persona stay.");
    expect(cleared).toContain("Tasks already running are left as they are.");
    await waitFor(() =>
      expect(fake.sent("revoke_dispatch_to_any")).toEqual([
        { plane: PLANE, asking: "qa", level: "you" },
      ]),
    );
    expect(
      await screen.findByRole("button", {
        name: "Allow for me: qa may dispatch to any persona",
      }),
    ).toBeInTheDocument();
  });

  it("shows a teammate's as waiting, with Accept and Not on my machine", async () => {
    const fake = core({
      standing: {
        any: [{ asking: "steward", level: "project", waiting: true, declined: false, ...ANYWHERE }],
      },
    });
    fake.on("accept_project_dispatch", () => {});
    fake.on("decline_project_dispatch", () => {
      fake.held.standing = stands({
        any: [{ asking: "steward", level: "project", waiting: true, declined: true, ...ANYWHERE }],
      });
    });
    render(<Table />);

    const accept = await screen.findByRole("button", {
      name: "Accept: the project's any persona for steward, on this machine",
    });
    expect(rowOf(accept)).toHaveTextContent("The project: allowed");
    expect(rowOf(accept)).toHaveTextContent(
      "Waiting for you: it is the project's, and it allows nothing on this machine until you accept it.",
    );

    const said = await press(
      "Accept: the project's any persona for steward, on this machine",
      "Accept",
    );
    expect(said).toContain("and to any persona added later.");
    await waitFor(() =>
      expect(fake.sent("accept_project_dispatch")).toEqual([
        { plane: PLANE, asking: "steward", target: "*" },
      ]),
    );

    await press(
      "Not on my machine: stop following the project's any persona for steward",
      "Not on my machine",
    );
    await waitFor(() =>
      expect(fake.sent("decline_project_dispatch")).toEqual([
        { plane: PLANE, asking: "steward", target: "*" },
      ]),
    );
    expect(
      await screen.findByText(/Not followed on this machine: you said so\./),
    ).toBeInTheDocument();
  });
});

describe("the pairs said never to, and the ones kept blocked for a chat", () => {
  const NEVER = { asking: "steward", target: "devops" };

  it("draws each never as a row with Lift, and says it holds down a chain", async () => {
    const fake = core({
      grants: [MINE],
      standing: { nevers: [NEVER, { asking: "qa", target: "devops" }] },
    });
    fake.on("lift_dispatch_never", ({ asking, target }) => {
      fake.held.standing = stands({
        nevers: fake.held.standing.nevers.filter(
          (one) => !(one.asking === asking && one.target === target),
        ),
      });
    });
    render(<Table />);

    const lift = await screen.findByRole("button", {
      name: "Lift: never for steward dispatching to devops",
    });
    expect(rowOf(lift)).toHaveTextContent("Never");
    expect(rowOf(lift)).toHaveTextContent(
      "You said so on this machine. No grant covers it, and no steward chat is asked. It also holds for a chain that starts from steward: a chat working for a steward chat does not dispatch to devops either.",
    );

    const said = await press("Lift: never for steward dispatching to devops", "Lift");

    expect(said).toContain("where none does the next dispatch asks you.");
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: "Lift: never for steward dispatching to devops" }),
      ).toBeNull(),
    );
    expect(fake.sent("lift_dispatch_never")).toEqual([
      { plane: PLANE, asking: "steward", target: "devops" },
    ]);
    // The other never, and the grant the lifted one was beating, are as they were.
    expect(
      screen.getByRole("button", { name: "Lift: never for qa dispatching to devops" }),
    ).toBeInTheDocument();
    expect(fake.sent("revoke_dispatch_grant")).toEqual([]);
    expect(
      screen.getByRole("button", { name: "Revoke: my grant for steward to devops" }),
    ).toBeInTheDocument();
  });

  it("says the core's refusal of a Lift and keeps the never drawn", async () => {
    const fake = core({ standing: { nevers: [NEVER] } });
    fake.on("lift_dispatch_never", () => {
      throw "purlis's event log is not open on this machine, so nothing was changed";
    });
    render(<Table />);

    await press("Lift: never for steward dispatching to devops", "Lift");

    expect(
      await screen.findByText(
        "purlis's event log is not open on this machine, so nothing was changed",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Lift: never for steward dispatching to devops" }),
    ).toBeInTheDocument();
  });

  it("draws a pair kept blocked for one chat read-only, with the chat", async () => {
    core({
      standing: { kept_blocked: [{ chat: "steward 3", asking: "steward", target: "qa" }] },
    });
    render(<Table />);

    const cell = await screen.findByText("Kept blocked in steward 3");
    const row = cell.closest("tr") as HTMLElement;
    expect(within(row).getByRole("rowheader")).toHaveTextContent("qa");
    expect(row).toHaveTextContent(
      "For that chat only, until it closes. Other chats are still asked.",
    );
    expect(row).toHaveTextContent("Ends with the chat");
    expect(within(row).queryByRole("button")).toBeNull();
  });
});

describe("where the list of nevers does not read", () => {
  const unread =
    "purlis could not read the list of pairs you said never to (.purlis/app/dispatch-never.json in this project), so it changed nothing there and no dispatch grant counts until it reads. Fix that file, or delete it to say never to nothing.";

  it("says so at the top with what to do, draws no never, and no grant pretends to count", async () => {
    const fake = core({
      grants: [MINE, OURS],
      // A core that answered a never anyway is not believed while the list does not read.
      standing: { nevers_unread: unread, nevers: [{ asking: "steward", target: "qa" }] },
    });
    render(<Table />);

    const alert = (await screen.findByText(new RegExp("^purlis could not read the list"))).closest(
      "[data-cause]",
    ) as HTMLElement;
    expect(alert).toHaveAttribute("role", "status");
    expect(alert).toHaveTextContent(unread);
    expect(alert).toHaveTextContent(
      "Until then purlis cannot say which pairs you said never to, so none is listed below and every dispatch to another persona asks you.",
    );
    const whole = await table();
    // The top of the section: before the table.
    expect(alert.compareDocumentPosition(whole) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByRole("button", { name: /^Lift: never/ })).toBeNull();
    expect(whole).not.toHaveTextContent("Never");
    for (const name of [
      "Revoke: my grant for steward to devops",
      "Remove for everyone: the project's grant for steward to devops",
    ])
      expect(rowOf(screen.getByRole("button", { name }))).toHaveTextContent(
        "Does not count until the list above reads.",
      );

    // Once the person mended the file, Read again draws what it holds.
    fake.held.standing = stands({ nevers: [{ asking: "steward", target: "qa" }] });
    await userEvent.setup().click(within(alert).getByRole("button", { name: "Read again" }));
    expect(
      await screen.findByRole("button", { name: "Lift: never for steward dispatching to qa" }),
    ).toBeInTheDocument();
    expect(screen.queryByText(/^purlis could not read the list/)).toBeNull();
  });
});

describe("while purlis cannot read the project's history", () => {
  it("says so at the top, and no grant of the project's accepted here says it counts", async () => {
    // #1543: the first moments after a launch, and a history git cannot be asked of.
    const fake = core({
      grants: [MINE, OURS, THEIRS],
      standing: {
        project_unsettled: true,
        any: [{ asking: "qa", level: "project", waiting: false, declined: false, ...ANYWHERE }],
      },
    });
    render(<Table />);

    const notice = (
      await screen.findByText(/^purlis has not read this project's git history/)
    ).closest("[data-cause]") as HTMLElement;
    expect(notice).toHaveAttribute("data-cause", "dispatch-project-unsettled");
    expect(notice).toHaveTextContent(
      "Until it can, no grant of the project's that you accepted counts on this machine",
    );
    const whole = await table();
    expect(notice.compareDocumentPosition(whole) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const accepted = "Accepted, but does not count until purlis can read this project's history.";
    expect(
      rowOf(
        screen.getByRole("button", {
          name: "Remove for everyone: the project's grant for steward to devops",
        }),
      ),
    ).toHaveTextContent(accepted);
    expect(
      rowOf(
        screen.getByRole("button", {
          name: "Remove for everyone: any persona for qa, in this project",
        }),
      ),
    ).toHaveTextContent(accepted);
    // Mine counts as it did, and a teammate's that waits says it waits.
    expect(
      rowOf(screen.getByRole("button", { name: "Revoke: my grant for steward to devops" })),
    ).not.toHaveTextContent(accepted);
    expect(
      rowOf(
        screen.getByRole("button", {
          name: "Accept: the project's grant for qa to devops, on this machine",
        }),
      ),
    ).not.toHaveTextContent(accepted);

    // Once a settling lands, Read again draws it as counting.
    fake.held.standing = stands({
      any: [{ asking: "qa", level: "project", waiting: false, declined: false, ...ANYWHERE }],
    });
    await userEvent.setup().click(within(notice).getByRole("button", { name: "Read again" }));
    await waitFor(() =>
      expect(screen.queryByText(/^purlis has not read this project's git history/)).toBeNull(),
    );
    expect(screen.queryByText(accepted)).toBeNull();
  });
});

describe("a persona that is away, and another under its name", () => {
  const AWAY =
    "Not in force while devops is not a persona of this project. Nothing was moved: it counts again when devops is back.";

  it("draws a grant naming a persona the project does not have as not in force, and moves nothing", async () => {
    const fake = core({
      grants: [MINE, OURS],
      standing: {
        personas: ["qa", "steward"],
        any: [{ asking: "devops", level: "you", waiting: false, declined: false, ...ANYWHERE }],
        nevers: [{ asking: "steward", target: "devops" }],
      },
    });
    render(<Table />);

    const whole = await table();
    const mine = rowOf(
      within(whole).getByRole("button", { name: "Revoke: my grant for steward to devops" }),
    );
    expect(mine).toHaveClass("dispatch-dormant");
    expect(mine).toHaveTextContent(AWAY);
    // The project's: it can be taken back, and is not offered to be accepted.
    const ours = rowOf(
      within(whole).getByRole("button", {
        name: "Remove for everyone: the project's grant for steward to devops",
      }),
    );
    expect(ours).toHaveTextContent(
      "It names devops, which is not a persona of this project now, so it allows nothing.",
    );
    expect(within(whole).queryByRole("button", { name: /^Accept/ })).toBeNull();
    // Its own group says so, offers no "any persona", and still lets what is there be cleared.
    expect(whole).toHaveTextContent(
      "devops Not a persona of this project now, so nothing here allows anything.",
    );
    expect(
      within(whole).queryByRole("button", { name: /devops may dispatch to any persona/ }),
    ).toBeNull();
    const any = rowOf(
      within(whole).getByRole("button", {
        name: "Clear: any persona for devops, for me on this machine",
      }),
    );
    expect(any).toHaveTextContent("Not in force while devops is not a persona of this project.");
    // A never keeps holding, and says whose name it is waiting on.
    expect(
      rowOf(
        within(whole).getByRole("button", {
          name: "Lift: never for steward dispatching to devops",
        }),
      ),
    ).toHaveTextContent(
      "devops is not a persona of this project now: the never still holds, and will hold for a persona made under that name.",
    );
    // Nothing is set aside, nothing to give back, and drawing it sent no write.
    expect(whole).not.toHaveTextContent("Set aside");
    expect(within(whole).queryByRole("button", { name: /^Give back/ })).toBeNull();
    expect(fake.wrote()).toEqual([]);
  });

  it("gives back to a persona that has the name of one that was gone, with one press for the name", async () => {
    const fake = core({
      grants: [grant({ id: "you\u001fdevops\u001fqa", asking: "devops", target: "qa" })],
      standing: {
        returned: ["devops"],
        back: ["devops"],
        nevers: [{ asking: "steward", target: "devops" }],
        dormant: [
          { asking: "steward", target: "devops", any: false, was: "devops" },
          { asking: "devops", target: "*", any: true, was: "devops" },
        ],
      },
    });
    fake.on("give_back_dispatch", () => {
      fake.held.standing = stands({ nevers: [{ asking: "steward", target: "devops" }] });
    });
    render(<Table />);

    const whole = await table();
    // A grant never moved is not in force either, and says what to do.
    expect(
      rowOf(within(whole).getByRole("button", { name: "Revoke: my grant for devops to qa" })),
    ).toHaveTextContent(
      "Not in force: devops was gone, and the persona of that name now is not the one that left. Give back to devops, or take this back.",
    );
    // What was set aside is listed, greyed, each with its own Remove and no Give back of its own.
    const aside = rowOf(
      within(whole).getByRole("button", {
        name: "Remove: the grant set aside for steward to devops",
      }),
    );
    expect(aside).toHaveClass("dispatch-dormant");
    expect(aside).toHaveTextContent(
      "It was an earlier devops's. It allows nothing unless you give it back, with Give back to devops.",
    );
    expect(within(whole).getAllByRole("button", { name: /^Give back/ })).toHaveLength(1);
    // The never is said to be an earlier persona's.
    expect(
      rowOf(
        within(whole).getByRole("button", {
          name: "Lift: never for steward dispatching to devops",
        }),
      ),
    ).toHaveTextContent(
      "It was said of an earlier persona named devops, and still holds for this one.",
    );

    const said = await press(
      "Give back to devops: what an earlier persona of this name was allowed",
      "Give back to devops",
    );

    expect(said).toContain(
      "Let the devops this project has now have what an earlier persona named devops was allowed? 2 grants set aside for it come back where the other persona exists",
    );
    await waitFor(() =>
      expect(fake.sent("give_back_dispatch")).toEqual([{ plane: PLANE, name: "devops" }]),
    );
    expect(await screen.findByText("Given back to devops.")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("button", { name: /^Give back/ })).toBeNull());
    expect(screen.queryByText("Set aside")).toBeNull();
  });

  it("removes the one entry pressed, telling any persona from a pair that reads like it", async () => {
    const fake = core({
      standing: {
        personas: ["qa", "steward"],
        dormant: [
          { asking: "steward", target: "*", any: true, was: "steward" },
          { asking: "steward", target: "*", any: false, was: "steward" },
          { asking: "qa", target: "devops", any: false, was: "devops" },
        ],
      },
    });
    fake.on("remove_dormant_dispatch", () => {});
    render(<Table />);

    const whole = await table();
    expect(
      rowOf(
        within(whole).getByRole("button", { name: "Remove: the grant set aside for qa to devops" }),
      ),
    ).toHaveTextContent(
      "It was an earlier devops's, and devops is not a persona of this project now. It allows nothing.",
    );
    await press("Remove: the grant set aside for steward to any persona", "Remove");
    await press("Remove: the grant set aside for steward to *", "Remove");

    expect(fake.sent("remove_dormant_dispatch")).toEqual([
      { plane: PLANE, asking: "steward", target: "*", any: true },
      { plane: PLANE, asking: "steward", target: "*", any: false },
    ]);
  });
});

describe("what a press says once it is done", () => {
  it("does not say a grant counts while a never covers the pair or the list of nevers does not read", async () => {
    const fake = core({
      grants: [THEIRS],
      standing: { nevers: [{ asking: "qa", target: "devops" }] },
    });
    fake.on("accept_project_dispatch", () => {});
    fake.on("allow_dispatch_to_any", () => {});
    render(<Table />);

    await press("Accept: the project's grant for qa to devops, on this machine", "Accept");
    expect(
      await screen.findByText(
        "Accepted on this machine: the project's grant for qa to devops. You said never to this pair, so it does not count until you lift that.",
      ),
    ).toBeInTheDocument();

    await press("Allow for me: qa may dispatch to any persona", "Allow for me");
    expect(
      await screen.findByText(
        "Allowed for you on this machine: qa to any persona. It covers personas added later. Where you said never for qa, the never still holds.",
      ),
    ).toBeInTheDocument();
  });
});

describe("where what stands could not be read", () => {
  it("says so, draws no table as if nothing stood, and reads again on a press", async () => {
    let fails = true;
    mockIPC((cmd) => {
      if (cmd === "dispatch_grants") return state({ grants: [MINE] });
      if (cmd === "dispatch_standing") {
        if (fails) throw "the project is not open";
        return stands();
      }
      return null;
    });
    render(<Table />);

    const notice = (
      await screen.findByText(/^purlis could not read what stands of dispatch here/)
    ).closest("[data-cause]") as HTMLElement;
    expect(notice).toHaveTextContent("(the project is not open)");
    expect(notice).toHaveTextContent(
      "The table is not drawn, since it could not say which grants count. Nothing was changed.",
    );
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.queryByRole("button", { name: /^Revoke/ })).toBeNull();

    fails = false;
    await userEvent.setup().click(within(notice).getByRole("button", { name: "Read again" }));

    expect(await table()).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Revoke: my grant for steward to devops" }),
    ).toBeInTheDocument();
    expect(screen.queryByText(/^purlis could not read what stands/)).toBeNull();
  });
});

describe("with the keyboard, and to a screen reader", () => {
  it("answers a question with Enter, puts it away with Escape, and keeps the focus in the table", async () => {
    const fake = core({ grants: [MINE] });
    fake.on("revoke_dispatch_grant", () => {
      fake.held.grants = [];
    });
    render(<Table />);
    const user = userEvent.setup();
    const revoke = await screen.findByRole("button", {
      name: "Revoke: my grant for steward to devops",
    });

    revoke.focus();
    await user.keyboard("{Enter}");
    const yes = within(screen.getByRole("group", { name: "Confirm" })).getByRole("button", {
      name: "Revoke",
    });
    expect(yes).toHaveFocus();
    // The question is what the confirming button is described by.
    expect(yes).toHaveAccessibleDescription(/Tasks already running are left as they are\./);

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("group", { name: "Confirm" })).toBeNull();
    expect(
      screen.getByRole("button", { name: "Revoke: my grant for steward to devops" }),
    ).toHaveFocus();
    expect(fake.wrote()).toEqual([]);

    await user.keyboard("{Enter}");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(fake.sent("revoke_dispatch_grant")).toHaveLength(1));
    // The row is gone: the focus is on the button now at its place, never lost to the page
    // and never sent back to the top of the table.
    await waitFor(() =>
      expect(document.activeElement).toHaveAccessibleName(
        "Allow for me: steward may dispatch to any persona",
      ),
    );
  });

  it("draws the question as a row of its own under the row it is about, the table's whole width", async () => {
    core({ grants: [MINE, OURS] });
    render(<Table />);
    const user = userEvent.setup();
    const revoke = await screen.findByRole("button", {
      name: "Revoke: my grant for steward to devops",
    });
    const row = rowOf(revoke);
    expect(within(row).getByRole("rowheader")).toHaveAttribute("rowspan", "2");

    await user.click(revoke);

    const question = screen.getByRole("group", { name: "Confirm" });
    const own = question.closest("tr") as HTMLElement;
    expect(own).not.toBe(row);
    expect(row.nextElementSibling).toBe(own);
    expect(within(own).getAllByRole("cell")).toHaveLength(1);
    expect(within(own).getByRole("cell")).toHaveAttribute("colspan", "4");
    // The target's name still runs down the side of all its rows, the question's included.
    expect(within(row).getByRole("rowheader")).toHaveAttribute("rowspan", "3");
    // The buttons of the row stay where they were, and say a question is open under them.
    expect(revoke).toHaveAttribute("aria-expanded", "true");
    expect(row).not.toContainElement(question);
  });

  it("puts the focus back on the row acted on where it is still there", async () => {
    const fake = core({ grants: [OURS] });
    fake.on("decline_project_dispatch", () => {
      fake.held.grants = [{ ...OURS, waiting: true, declined: true }];
    });
    render(<Table />);

    await press(
      "Not on my machine: stop following the project's grant for steward to devops",
      "Not on my machine",
    );

    await waitFor(() =>
      expect(document.activeElement).toHaveAccessibleName(
        "Accept: the project's grant for steward to devops, on this machine",
      ),
    );
  });

  it("names every button with the words it shows first, so saying them picks it", async () => {
    core({
      grants: [MINE, OURS, THEIRS],
      standing: {
        any: [{ asking: "qa", level: "project", waiting: true, declined: false, ...ANYWHERE }],
        nevers: [{ asking: "qa", target: "steward" }],
        back: ["devops"],
        dormant: [{ asking: "qa", target: "devops", any: false, was: "devops" }],
      },
    });
    render(<Table />);
    await table();

    const buttons = screen.getAllByRole("button");
    expect(buttons.length).toBeGreaterThan(12);
    for (const button of buttons) {
      const shown = button.textContent ?? "";
      expect(shown).not.toBe("");
      expect(button.getAttribute("aria-label")?.startsWith(`${shown}: `)).toBe(true);
    }
    const shown = new Set(buttons.map((one) => one.textContent));
    for (const words of [
      "Not on my machine",
      "Remove for everyone",
      "Allow for me",
      "Allow for everyone",
      "Accept",
      "Revoke",
      "Lift",
      "Remove",
      "Give back to devops",
    ])
      expect(shown).toContain(words);
  });

  it("reaches every button by Tab, each with a name that says whose grant it is", async () => {
    core({ grants: [MINE, OURS], standing: { nevers: [{ asking: "qa", target: "devops" }] } });
    render(<Table />);
    await table();
    const user = userEvent.setup();
    const buttons = screen.getAllByRole("button");

    const reached: (string | null)[] = [];
    for (let at = 0; at < buttons.length; at += 1) {
      await user.tab();
      reached.push(document.activeElement?.getAttribute("aria-label") ?? null);
    }

    expect(reached).toEqual(buttons.map((one) => one.getAttribute("aria-label")));
    expect(new Set(reached).size).toBe(reached.length);
    for (const name of reached) expect(name).toMatch(/steward|qa|devops/);
  });
});

/**
 * **Which workspace a grant holds in** (#1505): the column says it for every grant, the person
 * changes it there for a grant of their own or of the project's, and a grant whose workspace
 * is gone says it covers nothing.
 */
describe("which workspace a grant holds in", () => {
  const WORKSPACES = ["runners", "web"];
  const IN_RUNNERS = grant({
    id: "in\u001fyou\u001fsteward\u001fdevops\u001frunners",
    workspace: "runners",
  });
  const workspaceOf = (name: string) => screen.findByRole("combobox", { name });

  it("says where each grant holds: any workspace, one workspace, or where one chat's task works", async () => {
    core({
      grants: [
        MINE,
        grant({ ...IN_RUNNERS, target: "qa", id: "in\u001fyou\u001fsteward\u001fqa\u001frunners" }),
        grant({
          ...CHAT,
          target: "prod",
          workspace: "web",
          id: "chat\u001fc1\u001fsteward\u001fprod\u001fweb",
        }),
        grant({ ...CHAT, id: "chat\u001fc1\u001fsteward\u001fqa\u001f" }),
      ],
      standing: { workspaces: WORKSPACES, personas: ["devops", "prod", "qa", "steward"] },
    });
    render(<Table />);

    expect(await workspaceOf("Workspace: my grant for steward to devops")).toHaveValue("");
    expect(await workspaceOf("Workspace: my grant for steward to qa")).toHaveValue("runners");
    // A grant made for one chat is for the task it was allowed for, and is not changed here.
    const inWeb = rowOf(
      screen.getByRole("button", { name: "Revoke: this chat's grant for steward to prod in web" }),
    );
    expect(inWeb).toHaveTextContent("In web");
    expect(within(inWeb).queryByRole("combobox")).toBeNull();
    const atRoot = rowOf(
      screen.getByRole("button", { name: "Revoke: this chat's grant for steward to qa" }),
    );
    expect(atRoot).toHaveTextContent("At the project's root");
    expect(dispatchWorkspaceSaid(null)).toBe("Any workspace");
    expect(dispatchWorkspaceSaid("runners")).toBe("In runners");
  });

  it("narrows my grant to a workspace only once I confirm, and says what stops being covered", async () => {
    const fake = core({ grants: [MINE], standing: { workspaces: WORKSPACES } });
    fake.on("set_dispatch_workspace", () => {
      fake.held.grants = [IN_RUNNERS];
    });
    render(<Table />);

    const user = userEvent.setup();
    const list = await workspaceOf("Workspace: my grant for steward to devops");
    await user.selectOptions(list, "runners");
    // Nothing is sent, and the list still says what is true, until the question is answered.
    expect(fake.wrote()).toEqual([]);
    expect(list).toHaveValue("");
    const question = screen.getByRole("group", { name: "Confirm" });
    expect(question).toHaveTextContent(
      "Limit this grant to runners? steward chats will dispatch to devops without asking you only for work in runners. For work anywhere else the next dispatch asks you. Tasks already running are left as they are.",
    );
    await user.click(within(question).getByRole("button", { name: "Limit to runners" }));

    await waitFor(() =>
      expect(fake.sent("set_dispatch_workspace")).toEqual([
        {
          plane: PLANE,
          asking: "steward",
          target: "devops",
          level: "you",
          from: null,
          to: "runners",
        },
      ]),
    );
    expect(await workspaceOf("Workspace: my grant for steward to devops")).toHaveValue("runners");
    expect(await screen.findByText(/It now holds in runners only\./)).toBeInTheDocument();
  });

  it("puts a change away on Cancel and sends nothing", async () => {
    const fake = core({ grants: [IN_RUNNERS], standing: { workspaces: WORKSPACES } });
    render(<Table />);

    const user = userEvent.setup();
    const list = await workspaceOf("Workspace: my grant for steward to devops");
    await user.selectOptions(list, "");
    const question = screen.getByRole("group", { name: "Confirm" });
    expect(question).toHaveTextContent(
      "Let this grant hold in any workspace? steward chats will dispatch to devops for work in every workspace of this project, and at its root, without asking you.",
    );
    await user.click(within(question).getByRole("button", { name: "Cancel" }));
    expect(fake.wrote()).toEqual([]);
    expect(list).toHaveValue("runners");
    await waitFor(() => expect(list).toHaveFocus());
  });

  it("changes a project grant's workspace for everyone, saying it edits the committed file first", async () => {
    const ours = grant({
      id: "in\u001fproject\u001fsteward\u001fdevops\u001frunners",
      level: "project",
      by: "Dana",
      workspace: "runners",
    });
    const fake = core({ grants: [ours], standing: { workspaces: WORKSPACES } });
    fake.on("set_dispatch_workspace", () => {});
    render(<Table />);

    const user = userEvent.setup();
    await user.selectOptions(
      await workspaceOf("Workspace: the project's grant for steward to devops"),
      "web",
    );
    const question = screen.getByRole("group", { name: "Confirm" });
    expect(question).toHaveTextContent(
      "This changes purlis.toml, the project's committed file, for everyone: your teammates get it when they pull it, and each accepts it on their own machine.",
    );
    await user.click(within(question).getByRole("button", { name: "Limit to web" }));
    await waitFor(() =>
      expect(fake.sent("set_dispatch_workspace")).toEqual([
        {
          plane: PLANE,
          asking: "steward",
          target: "devops",
          level: "project",
          from: "runners",
          to: "web",
        },
      ]),
    );
  });

  it("draws a grant whose workspace is gone as covering nothing, with Remove", async () => {
    const gone = grant({
      id: "in\u001fyou\u001fsteward\u001fdevops\u001fold",
      workspace: "old",
      nowhere:
        "old is not a workspace of this project now, so this grant covers nothing. A grant does not follow a workspace that was renamed: set its workspace again, or remove it.",
    });
    const fake = core({ grants: [gone], standing: { workspaces: WORKSPACES } });
    fake.on("revoke_dispatch_grant", () => {
      fake.held.grants = [];
    });
    render(<Table />);

    const remove = await screen.findByRole("button", {
      name: "Remove: my grant for steward to devops in old",
    });
    const row = rowOf(remove);
    expect(row).toHaveClass("dispatch-dormant");
    expect(row).toHaveTextContent(
      "A grant does not follow a workspace that was renamed: set its workspace again, or remove it.",
    );
    // There is no workspace of that name to count it for.
    expect(screen.queryByRole("button", { name: /Count it again/ })).toBeNull();
    const said = await press("Remove: my grant for steward to devops in old", "Remove");
    expect(said).toContain("It covers nothing now, so nothing changes for any chat.");
    await waitFor(() =>
      expect(fake.sent("revoke_dispatch_grant")).toEqual([{ plane: PLANE, id: gone.id }]),
    );
  });

  it("offers Count it again where a workspace was made again under the name", async () => {
    const stale = grant({
      ...IN_RUNNERS,
      nowhere:
        "A workspace named runners was removed or renamed after this grant was made, so it covers nothing in the one that is there now.",
    });
    const fake = core({ grants: [stale], standing: { workspaces: WORKSPACES } });
    fake.on("set_dispatch_workspace", () => {
      fake.held.grants = [IN_RUNNERS];
    });
    render(<Table />);

    const said = await press(
      "Count it again: my grant for steward to devops in runners",
      "Count it again",
    );
    expect(said).toContain(
      "Let this grant hold in the workspace named runners that is there now? It was made for an earlier workspace of that name.",
    );
    await waitFor(() =>
      expect(fake.sent("set_dispatch_workspace")).toEqual([
        {
          plane: PLANE,
          asking: "steward",
          target: "devops",
          level: "you",
          from: "runners",
          to: "runners",
        },
      ]),
    );
    expect(
      await screen.findByRole("button", {
        name: "Revoke: my grant for steward to devops in runners",
      }),
    ).toBeInTheDocument();
  });

  it("counts a project grant again on this machine without touching the committed file", async () => {
    const stale = grant({
      id: "in\u001fproject\u001fsteward\u001fdevops\u001frunners",
      level: "project",
      by: "Dana",
      workspace: "runners",
      nowhere:
        "A workspace named runners was removed or renamed after this grant was made, so it covers nothing in the one that is there now.",
    });
    const fake = core({ grants: [stale], standing: { workspaces: WORKSPACES } });
    fake.on("accept_project_dispatch_in", () => {});
    render(<Table />);

    const said = await press(
      "Count it again: the project's grant for steward to devops in runners",
      "Count it again",
    );
    expect(said).toContain("Follow this grant on this machine for the workspace named runners");
    expect(said).toContain("purlis.toml is not changed.");
    await waitFor(() =>
      expect(fake.sent("accept_project_dispatch_in")).toEqual([
        { plane: PLANE, asking: "steward", target: "devops", workspace: "runners" },
      ]),
    );
    // Never the command that edits the project's file.
    expect(fake.sent("set_dispatch_workspace")).toEqual([]);
  });

  it("accepts a teammate's grant for one workspace for that workspace", async () => {
    const theirs = grant({
      id: "in\u001fproject\u001fqa\u001fdevops\u001fweb",
      asking: "qa",
      level: "project",
      by: "Dana",
      waiting: true,
      workspace: "web",
    });
    const fake = core({ grants: [theirs], standing: { workspaces: WORKSPACES } });
    fake.on("accept_project_dispatch_in", () => {});
    render(<Table />);

    const said = await press(
      "Accept: the project's grant for qa to devops in web, on this machine",
      "Accept",
    );
    expect(said).toContain(
      "qa chats will dispatch to devops for work in web here without asking you, and for work anywhere else they still ask.",
    );
    await waitFor(() =>
      expect(fake.sent("accept_project_dispatch_in")).toEqual([
        { plane: PLANE, asking: "qa", target: "devops", workspace: "web" },
      ]),
    );
    expect(fake.sent("accept_project_dispatch")).toEqual([]);
  });

  it("offers no Not on my machine for a teammate's grant for one workspace nobody here accepted", async () => {
    // #1543: it already allows nothing here, and there is nothing of this machine's to stop.
    const theirs = grant({
      id: "in\u001fproject\u001fqa\u001fdevops\u001fweb",
      asking: "qa",
      level: "project",
      by: "Dana",
      waiting: true,
      workspace: "web",
    });
    core({ grants: [theirs], standing: { workspaces: WORKSPACES } });
    render(<Table />);

    const accept = await screen.findByRole("button", {
      name: "Accept: the project's grant for qa to devops in web, on this machine",
    });
    expect(within(rowOf(accept)).queryByRole("button", { name: /^Not on my machine/ })).toBeNull();
    expect(
      within(rowOf(accept)).getByRole("button", { name: /^Remove for everyone/ }),
    ).toBeInTheDocument();
  });

  it("draws any persona limited to a workspace as a line of its own, cleared by its id", async () => {
    const fake = core({
      standing: {
        workspaces: WORKSPACES,
        any: [
          {
            asking: "qa",
            level: "you",
            waiting: false,
            declined: false,
            workspace: "web",
            nowhere: null,
            id: "in\u001fyou\u001fqa\u001f*\u001fweb",
          },
        ],
      },
    });
    fake.on("revoke_dispatch_grant", () => {});
    render(<Table />);

    expect(await workspaceOf("Workspace: my grant for qa to any persona")).toHaveValue("web");
    // Beside it, "any persona" in any workspace is still not allowed, and can be.
    expect(
      screen.getByRole("button", { name: "Allow for me: qa may dispatch to any persona" }),
    ).toBeInTheDocument();
    await press("Clear: any persona for qa in web, for me on this machine", "Clear");
    await waitFor(() =>
      expect(fake.sent("revoke_dispatch_grant")).toEqual([
        { plane: PLANE, id: "in\u001fyou\u001fqa\u001f*\u001fweb" },
      ]),
    );
  });

  it("says a never holds in every workspace", async () => {
    core({
      standing: { workspaces: WORKSPACES, nevers: [{ asking: "steward", target: "devops" }] },
    });
    render(<Table />);

    const lift = await screen.findByRole("button", {
      name: "Lift: never for steward dispatching to devops",
    });
    expect(rowOf(lift)).toHaveTextContent("Every workspace");
    expect(within(rowOf(lift)).queryByRole("combobox")).toBeNull();
  });
});

describe("what policy locks", () => {
  it("draws a grant policy locks as locked, with who locked it and nothing to press", async () => {
    const locked =
      "Policy forbids steward chats dispatching to devops. Locked by policy, set by IT in /etc/purlis/policy.json.";
    mockIPC((cmd) => {
      if (cmd === "dispatch_grants")
        return state({
          grants: [{ ...MINE, locked }],
          locked_pairs: [{ asking: "steward", target: "devops" }],
          locked_by: "Locked by policy, set by IT in /etc/purlis/policy.json.",
        });
      if (cmd === "dispatch_standing") return stands({ personas: ["steward"] });
      return null;
    });
    render(<Table />);

    const row = (await screen.findByText(locked)).closest("tr") as HTMLElement;
    expect(row).toHaveTextContent("Locked by policy");
    expect(within(row).queryByRole("button")).toBeNull();
    const locks = screen.getByRole("list", { name: "Locked by policy" });
    expect(within(locks).getByRole("listitem")).toHaveTextContent("steward to devops");
    expect(
      screen.getByText("Locked by policy, set by IT in /etc/purlis/policy.json."),
    ).toBeInTheDocument();
  });

  it("says a lock on all dispatch, and who set it", async () => {
    const all =
      "Policy forbids one chat dispatching to another. Locked by policy, set by IT in /etc/purlis/policy.json.";
    mockIPC((cmd) =>
      cmd === "dispatch_grants"
        ? state({
            all_locked: all,
            locked_by: "Locked by policy, set by IT in /etc/purlis/policy.json.",
          })
        : null,
    );
    render(<Table />);

    expect(await screen.findByText(all)).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Locked by policy" })).toBeNull();
  });
});

describe("Settings' Granted page", () => {
  it("says where the dispatch grants are with a link there, and lists and changes none itself", async () => {
    const fake = core({ grants: [MINE] });
    const setting = grantedGroup(PLANE, FILE).settings.find(
      (one) => one.id === "project.sandbox.granted.dispatch",
    ) as LiveSetting;
    expect(setting.label).toBe("Who may dispatch to whom");
    function Row() {
      const { control } = setting.useControl();
      const shown = useShownGroup(settingsPlace("project", PLANE));
      return (
        <>
          {control({ id: "g", labelledBy: "g-label" })}
          <output>{shown?.group ?? "nowhere yet"}</output>
        </>
      );
    }
    render(<Row />);

    expect(
      screen.getByText(/Dispatch grants are listed, revoked and lifted on the Dispatch page\./),
    ).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("nowhere yet");
    expect(screen.queryByRole("table")).toBeNull();

    await userEvent.setup().click(screen.getByRole("button", { name: "Open that page" }));

    // The Settings tab of this project is taken to its Dispatch page.
    expect(screen.getByRole("status")).toHaveTextContent("project.dispatch");
    expect(fake.asked).toEqual([]);
  });
});
