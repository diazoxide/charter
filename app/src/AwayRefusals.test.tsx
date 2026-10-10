import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { useState } from "react";
import type { AwayRefusal } from "./bindings";
import { awaySaid, awayRow, clipped, oftenSaid } from "./AwayRefusals";
import { useAwayRefusals } from "./dispatchAway";
import { Inbox, type UpdateRow } from "./Inbox";
import { awayUpdateKey } from "./inboxUpdates";
import { SETTLE_MS } from "./TaskBlocksNotice";

/**
 * **A dispatch refused while nobody was there, an update in the Inbox** (#1507, #1693): the
 * item, its three answers, a grant that is never sent from a row that just moved, and the pair
 * the person said never to by the time they look.
 */

const START = Date.parse("2026-10-11T10:00:00Z");
let user: ReturnType<typeof userEvent.setup>;

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(START);
  user = userEvent.setup();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.useRealTimers();
});

/** Time passes past the grant's guard, as a person reading the row lets it. */
const read = () => act(() => vi.setSystemTime(Date.now() + SETTLE_MS + 1));

const PLANE = "/home/dev/plane";
const NOW = 1_800_000_000;

const ALLOWS =
  "Allow from now on lets steward chats dispatch to devops without asking, for work in ide " +
  "only: this one pair, for you on this machine. A chat nobody is at may use it too. devops " +
  "works with its own access: vault team; hosts k8s.internal.example:6443, and may itself " +
  "dispatch to any persona. It starts nothing now. Revoke it in Settings › Project › Dispatch.";

const ALLOW =
  "Allow from now on: steward chats dispatch to devops, for you on this machine, for work in ide only";

const refusal = (over: Partial<AwayRefusal> = {}): AwayRefusal => ({
  asking: "steward",
  target: "devops",
  workspace: "ide",
  latest: NOW - 7200,
  times: 3,
  allows: ALLOWS,
  nowhere: null,
  shown: "away-s0",
  ...over,
});

/** The rows the Inbox draws for `refused`, each answered through `answer`. */
const rowsOf = (
  refused: readonly AwayRefusal[],
  answer: (one: AwayRefusal, how: "allow" | "dismiss" | "never") => void = () => {},
): UpdateRow[] =>
  refused.map((one) => ({
    update: {
      key: awayUpdateKey(one),
      kind: "refused-away",
      at: one.latest,
      session: null,
      chain: [],
      says: awaySaid(one),
      read: false,
    },
    ...awayRow(one, answer, () => {}),
  }));

/** The Inbox with these refused dispatches as its updates and no asks. */
function Updated({
  refused,
  answer,
}: {
  refused: readonly AwayRefusal[];
  answer?: (one: AwayRefusal, how: "allow" | "dismiss" | "never") => void;
}) {
  return (
    <Inbox
      plane={PLANE}
      asks={[]}
      onGo={() => {}}
      onLeave={() => {}}
      updates={{ rows: rowsOf(refused, answer), onMarkAllRead: () => {}, onDismissAll: () => {} }}
    />
  );
}

/** The update of the pair `said` names. */
const updateOf = (said: RegExp) =>
  screen.getAllByRole("listitem").find((row) => said.test(row.textContent ?? "")) as HTMLElement;

describe("what the item says", () => {
  it("says who wanted whom, and how often, for which workspace", () => {
    expect(awaySaid(refusal())).toBe("steward wanted devops while you were away");
    expect(oftenSaid(refusal())).toBe("Refused 3 times, for work in ide");
    expect(oftenSaid(refusal({ times: 1, workspace: null }))).toBe("Refused once");
    expect(oftenSaid(refusal({ times: 2 }))).toBe("Refused twice, for work in ide");
  });

  it("clips a long name and says it is cut", () => {
    expect(clipped("deploy")).toBe("deploy");
    expect(clipped("x".repeat(41))).toBe(`${"x".repeat(40)}…`);
  });

  it("has no place for a chat's words: an item is two personas, a workspace, a count and a time", () => {
    // `allows`, `nowhere` and `shown` are the core's own sentences and digest of them.
    expect(Object.keys(refusal()).sort()).toEqual([
      "allows",
      "asking",
      "latest",
      "nowhere",
      "shown",
      "target",
      "times",
      "workspace",
    ]);
  });
});

describe("a dispatch refused while nobody was there, in the Inbox's updates", () => {
  it("is an update of its own, with no Go: Never for this pair, Allow from now on, and Dismiss", () => {
    render(<Updated refused={[refusal()]} />);
    const row = updateOf(/steward wanted devops/);
    expect(
      within(row)
        .getAllByRole("button")
        .map((one) => one.textContent),
    ).toEqual(["Never for this pair", "Allow from now on", "Dismiss"]);
    expect(within(row).queryByRole("button", { name: /^Go to/ })).toBeNull();
  });

  it("says what Allow from now on allows and reaches, before the press and in the button's name", () => {
    render(<Updated refused={[refusal()]} />);
    const row = updateOf(/steward wanted devops/);
    // The core's sentence, whole: the pair, the workspace it holds in, a chat nobody is at,
    // and what the target works with, onward reach included.
    expect(row).toHaveTextContent(ALLOWS);
    const allow = within(row).getByRole("button", { name: ALLOW });
    expect(allow).toHaveAttribute("title", ALLOWS);
    expect(allow.getAttribute("aria-label")).not.toContain("every workspace");
    // Nothing wider is offered from here.
    expect(within(row).queryByRole("button", { name: /everyone|any persona/i })).toBe(null);
  });

  it("names the grant for a refusal at the project's root as one that holds in any workspace", () => {
    // #1505: there is no workspace to limit that grant to, and the button says what it is.
    const root = refusal({
      target: "qa",
      workspace: null,
      allows: ALLOWS.replace("for work in ide only", "in any workspace"),
    });
    render(<Updated refused={[refusal(), root]} />);
    expect(
      screen
        .getAllByRole("button", { name: /^Allow from now on/ })
        .map((one) => one.getAttribute("aria-label")),
    ).toEqual([
      ALLOW,
      "Allow from now on: steward chats dispatch to qa, for you on this machine, in any workspace",
    ]);
  });

  it("says on the button that Allow keeps nothing where the item's workspace is gone", () => {
    const gone = refusal({
      workspace: "old",
      nowhere: "old is not a workspace of this project now, so purlis keeps no grant for it.",
      allows:
        "old is not a workspace of this project now, so purlis keeps no grant for it. So Allow from now on keeps nothing for this item; put it away with Dismiss.",
    });
    render(<Updated refused={[gone]} />);
    const row = updateOf(/steward wanted devops/);
    expect(row).toHaveTextContent("So Allow from now on keeps nothing for this item");
    expect(
      within(row).getByRole("button", {
        name: "Allow from now on: steward chats dispatch to devops, for you on this machine, which keeps nothing: old is not a workspace of this project now",
      }),
    ).toBeInTheDocument();
  });

  it("answers with each of the three, each for its own item", async () => {
    const answered: string[] = [];
    const away = [
      refusal(),
      refusal({ target: "qa", workspace: null, times: 1 }),
      refusal({ asking: "qa", target: "prod" }),
    ];
    const said = (one: AwayRefusal) => `${one.asking}>${one.target}@${one.workspace ?? "root"}`;
    render(<Updated refused={away} answer={(one, how) => answered.push(`${how} ${said(one)}`)} />);
    await read();

    await user.click(
      screen.getByRole("button", { name: "Dismiss: steward wanted qa while you were away" }),
    );
    await user.click(
      screen.getByRole("button", {
        name: "Never for this pair: qa chats never dispatch to prod, for you on this machine",
      }),
    );
    await user.click(screen.getByRole("button", { name: ALLOW }));

    expect(answered).toEqual([
      "dismiss steward>qa@root",
      "never qa>prod@ide",
      "allow steward>devops@ide",
    ]);
  });
});

describe("a grant is never sent from a row that just moved", () => {
  it("sends no Allow pressed just after the row was drawn, and says why", async () => {
    const answered: string[] = [];
    render(<Updated refused={[refusal()]} answer={(_, how) => answered.push(how)} />);
    await user.click(screen.getByRole("button", { name: ALLOW }));
    expect(answered).toEqual([]);
    expect(screen.getByRole("status")).toHaveTextContent("nothing was allowed");

    await read();
    await user.click(screen.getByRole("button", { name: ALLOW }));
    expect(answered).toEqual(["allow"]);
  });

  it("sends no Allow pressed just after another update came in above it and moved it", async () => {
    const answered: string[] = [];
    function Moving() {
      const [refused, setRefused] = useState([refusal({ asking: "planner", target: "reviewer" })]);
      return (
        <>
          <Updated refused={refused} answer={(one, how) => answered.push(`${how} ${one.target}`)} />
          <button
            type="button"
            data-testid="came"
            onClick={() => setRefused((was) => [refusal({ target: "qa" }), ...was])}
          />
        </>
      );
    }
    render(<Moving />);
    await read();
    // A chat is refused again for another pair, which comes in above.
    await user.click(screen.getByTestId("came"));
    const allow = screen.getByRole("button", {
      name: /^Allow from now on: planner chats dispatch to reviewer/,
    });
    await user.click(allow);
    expect(answered).toEqual([]);

    await read();
    await user.click(allow);
    expect(answered).toEqual(["allow reviewer"]);
  });

  it("never lands the keyboard on Allow by itself: it comes in on the update", async () => {
    const answered: string[] = [];
    render(<Updated refused={[refusal()]} answer={(_, how) => answered.push(how)} />);
    await read();
    await user.tab();
    expect(document.activeElement?.tagName).not.toBe("BUTTON");
    await user.keyboard("{Enter}");
    expect(answered).toEqual([]);
  });
});

/** The list as the window holds it for one project, in the Inbox, with what was said. */
function Held() {
  const away = useAwayRefusals([PLANE]);
  const [said, setSaid] = useState<string[]>([]);
  const answer = (one: AwayRefusal, how: "allow" | "dismiss" | "never") =>
    void away[how](PLANE, one).then((answered) => {
      if (answered)
        setSaid((was) => [...was, `${answered.refused ? "refused" : "said"}: ${answered.words}`]);
    });
  return (
    <>
      <Updated refused={away.held[PLANE] ?? []} answer={answer} />
      <output>{said.join("\n")}</output>
    </>
  );
}

describe("the list, read from the core and answered through it", () => {
  /** The pairs drawn, top to bottom. */
  const rows = () =>
    screen.queryAllByRole("listitem").map((row) => row.querySelector(".ask-says")?.textContent);

  it("keeps the core's order, which no later refusal moves", async () => {
    mockIPC((cmd) =>
      cmd === "dispatch_away"
        ? [refusal({ latest: NOW - 9000 }), refusal({ target: "qa", latest: NOW - 10 })]
        : undefined,
    );
    render(<Held />);

    await screen.findByText("steward wanted qa while you were away");
    expect(rows()).toEqual([
      "steward wanted devops while you were away",
      "steward wanted qa while you were away",
    ]);
  });

  it("Allow from now on sends the one pair and its workspace, says the core's sentence, and the item goes", async () => {
    const asked: { cmd: string; args: unknown }[] = [];
    let listed = [refusal(), refusal({ target: "qa" })];
    mockIPC((cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "dispatch_away") return listed;
      if (cmd === "allow_dispatch_away") {
        listed = [refusal({ target: "qa" })];
        return { said: "Allowed for me on this machine.", refused: listed };
      }
      return undefined;
    });
    render(<Held />);

    const allow = await screen.findByRole("button", { name: ALLOW });
    await read();
    await user.click(allow);

    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent("said: Allowed for me on this machine."),
    );
    expect(asked.filter((one) => one.cmd === "allow_dispatch_away").map((one) => one.args)).toEqual(
      [{ plane: PLANE, asking: "steward", target: "devops", workspace: "ide", shown: "away-s0" }],
    );
    // No level and no wildcard is the window's to send.
    expect(asked.some((one) => /allow_dispatch$|allow_dispatch_to_any/.test(one.cmd))).toBe(false);
    await waitFor(() => expect(rows()).toEqual(["steward wanted qa while you were away"]));
  });

  it("Never for this pair sends the pair, says the core's sentence, and allows nothing", async () => {
    const asked: { cmd: string; args: unknown }[] = [];
    let listed = [refusal()];
    mockIPC((cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "dispatch_away") return listed;
      if (cmd === "never_dispatch_away") {
        listed = [];
        return { said: "No steward chat dispatches to devops on this machine.", refused: listed };
      }
      return undefined;
    });
    render(<Held />);

    await user.click(await screen.findByRole("button", { name: /^Never for this pair:/ }));

    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        "said: No steward chat dispatches to devops on this machine.",
      ),
    );
    expect(asked.filter((one) => one.cmd === "never_dispatch_away").map((one) => one.args)).toEqual(
      [{ plane: PLANE, asking: "steward", target: "devops", workspace: "ide" }],
    );
    expect(asked.some((one) => one.cmd === "allow_dispatch_away")).toBe(false);
    await waitFor(() => expect(rows()).toEqual([]));
  });

  it("Dismiss puts the item away and allows nothing", async () => {
    const asked: string[] = [];
    let listed = [refusal()];
    mockIPC((cmd) => {
      asked.push(cmd);
      if (cmd === "dispatch_away") return listed;
      if (cmd === "dismiss_dispatch_away") {
        listed = [];
        return listed;
      }
      return undefined;
    });
    render(<Held />);

    await user.click(
      await screen.findByRole("button", { name: /^Dismiss: steward wanted devops/ }),
    );

    await waitFor(() => expect(rows()).toEqual([]));
    expect(asked).toContain("dismiss_dispatch_away");
    expect(asked).not.toContain("allow_dispatch_away");
    expect(screen.getByRole("status")).toHaveTextContent("");
  });

  it("says the core's refusal and reads the list again when an Allow is no longer one to make", async () => {
    let listed = [refusal()];
    const never =
      "You said never to steward chats dispatching to devops on this machine, so nothing was allowed.";
    mockIPC((cmd) => {
      if (cmd === "dispatch_away") return listed;
      if (cmd === "allow_dispatch_away") {
        listed = [];
        throw never;
      }
      return undefined;
    });
    render(<Held />);

    const allow = await screen.findByRole("button", { name: ALLOW });
    await read();
    await user.click(allow);

    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(`refused: ${never}`));
    await waitFor(() => expect(screen.queryByRole("button")).toBeNull());
  });
});
