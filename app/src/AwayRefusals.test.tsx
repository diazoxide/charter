import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { useState } from "react";
import type { AwayRefusal } from "./bindings";
import type { Offer } from "./actions";
import { awaySaid, awayWhere, clipped, GONE, type AwayItem } from "./AwayRefusals";
import { useAwayRefusals } from "./dispatchAway";
import { NeedsYouMenu, type Needing } from "./NeedsYou";

/**
 * **A dispatch refused while nobody was there, in the title bar's needs-you list** (#1507):
 * the item, its three answers, a list that holds still while it is open, and the pair the
 * person said never to by the time they look.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";
const NOW = 1_800_000_000;

const ALLOWS =
  "Allow from now on lets steward chats dispatch to devops without asking: this one pair, for " +
  "you on this machine, in and into every workspace of this project. A chat nobody is at may " +
  "use it too. devops chats may themselves dispatch to any persona without asking. It starts " +
  "nothing now. Revoke it in Settings › Project › Dispatch.";

const ALLOW =
  "Allow from now on: steward chats dispatch to devops, for you on this machine, in every workspace of this project";

const refusal = (over: Partial<AwayRefusal> = {}): AwayRefusal => ({
  asking: "steward",
  target: "devops",
  workspace: "ide",
  latest: NOW - 7200,
  times: 3,
  allows: ALLOWS,
  ...over,
});

const item = (over: Partial<AwayRefusal> = {}): AwayItem => ({
  ...refusal(over),
  plane: PLANE,
  project: "charter",
});

/** A chat asking, as a project reports it. */
function chat(session: number): Needing {
  const name = `ide.${session}`;
  const offer = (id: string, title: string, does: Offer["does"]): Offer => ({
    id,
    title,
    available: true,
    reason: "",
    does,
    name,
  });
  return {
    plane: PLANE,
    session,
    name,
    workspace: "ide",
    project: "charter",
    go: offer(`needs.show:${session}`, `Show ${name}, which needs you`, {
      verb: "showChat",
      session,
    }),
    ignore: offer(`needs.ignore:${session}`, `Ignore ${name} until it asks again`, {
      verb: "ignoreNeedsYou",
      session,
    }),
  };
}

const hand = () => screen.getByRole("button", { name: /refused while you were away$/ });
/** The pairs drawn, top to bottom. */
const rows = () =>
  screen
    .getAllByRole("group")
    .map((group) => group.querySelector(".needs-you-name")?.textContent ?? "")
    .filter((said) => said.includes(" wanted "));

describe("what the item says", () => {
  it("says who wanted whom, how often and when, the workspace and the project", () => {
    expect(awaySaid(item())).toBe("steward wanted devops while you were away");
    expect(awayWhere(item(), NOW)).toBe("3 times, last 2h ago · ide · charter");
    expect(awayWhere(item({ times: 1, latest: NOW - 300 }), NOW)).toBe(
      "once, 5m ago · ide · charter",
    );
    expect(awayWhere(item({ times: 2, latest: NOW - 30, workspace: null }), NOW)).toBe(
      "twice, last 30s ago · charter",
    );
  });

  it("clips a long name and says it is cut", () => {
    expect(clipped("deploy")).toBe("deploy");
    expect(clipped("x".repeat(41))).toBe(`${"x".repeat(40)}…`);
  });

  it("has no place for a chat's words: an item is two personas, a workspace, a count and a time", () => {
    expect(Object.keys(refusal()).sort()).toEqual([
      "allows",
      "asking",
      "latest",
      "target",
      "times",
      "workspace",
    ]);
  });
});

describe("a dispatch refused while nobody was there, in the title bar's list", () => {
  it("is an item of its own: counted by the hand, with no Go and no chat to ignore", async () => {
    render(<NeedsYouMenu quiet={[]} items={[]} away={[item()]} onPress={() => {}} />);

    const button = screen.getByRole("button", {
      name: "1 dispatch was refused while you were away",
    });
    expect(button).toHaveTextContent("1");
    await userEvent.click(button);

    const group = await screen.findByRole("group", {
      name: /^steward wanted devops while you were away · 3 times, last .* ago · ide · charter$/,
    });
    expect(group).toHaveTextContent("steward wanted devops while you were away");
    expect(
      within(group)
        .getAllByRole("menuitem")
        .map((one) => one.textContent),
    ).toEqual(["Dismiss", "Never for this pair", "Allow from now on"]);
    expect(screen.queryByRole("menuitem", { name: /^Go to/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /^Ignore/ })).toBeNull();
  });

  it("says what Allow from now on allows and reaches, before the press and in the button's name", async () => {
    render(<NeedsYouMenu quiet={[]} items={[]} away={[item()]} onPress={() => {}} />);
    await userEvent.click(hand());

    const group = await screen.findByRole("group", { name: /^steward wanted devops/ });
    // The core's sentence, whole: the pair, every workspace, a chat nobody is at, and onward.
    expect(group).toHaveTextContent(ALLOWS);
    const allow = within(group).getByRole("menuitem", { name: ALLOW });
    expect(allow).toHaveAttribute("title", ALLOWS);
    expect(allow.getAttribute("aria-label")).toContain("in every workspace of this project");
    // Nothing wider is offered from here.
    expect(within(group).queryByRole("menuitem", { name: /everyone|any persona/i })).toBe(null);
  });

  it("answers with each of the three, each for its own item", async () => {
    const answered: string[] = [];
    const away = [
      item(),
      item({ target: "qa", workspace: null, times: 1 }),
      item({ asking: "qa", target: "prod" }),
    ];
    const said = (one: AwayItem) => `${one.asking}>${one.target}@${one.workspace ?? "root"}`;
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[]}
        away={away}
        onPress={() => {}}
        onAllowAway={(one) => answered.push(`allow ${said(one)}`)}
        onDismissAway={(one) => answered.push(`dismiss ${said(one)}`)}
        onNeverAway={(one) => answered.push(`never ${said(one)}`)}
      />,
    );

    await userEvent.click(hand());
    await userEvent.click(
      await screen.findByRole("menuitem", {
        name: "Dismiss: steward wanted qa while you were away",
      }),
    );
    await userEvent.click(hand());
    await userEvent.click(
      await screen.findByRole("menuitem", {
        name: "Never for this pair: qa chats never dispatch to prod, for you on this machine",
      }),
    );
    await userEvent.click(hand());
    await userEvent.click(await screen.findByRole("menuitem", { name: ALLOW }));

    expect(answered).toEqual([
      "dismiss steward>qa@root",
      "never qa>prod@ide",
      "allow steward>devops@ide",
    ]);
  });

  it("is counted beside the chats that ask, and is no chat's row", async () => {
    render(<NeedsYouMenu quiet={[]} items={[chat(3)]} away={[item()]} onPress={() => {}} />);

    await userEvent.click(screen.getByRole("button", { name: "2 things need you" }));

    expect(await screen.findByRole("menuitem", { name: "Go to ide.3 · ide · charter" })).toBe(
      screen.getAllByRole("menuitem")[0],
    );
    expect(screen.getByRole("group", { name: /^steward wanted devops/ })).toBeInTheDocument();
  });
});

describe("the keyboard never lands on Allow from now on by itself", () => {
  it("opens from the keyboard on Dismiss, and Allow is the last answer the arrows reach", async () => {
    const answered: string[] = [];
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[]}
        away={[item()]}
        onPress={() => {}}
        onAllowAway={() => answered.push("allow")}
        onDismissAway={() => answered.push("dismiss")}
      />,
    );
    hand().focus();

    await userEvent.keyboard("{Enter}");

    const dismiss = await screen.findByRole("menuitem", { name: /^Dismiss:/ });
    await waitFor(() => expect(dismiss).toHaveFocus());
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: /^Never for this pair:/ })).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: ALLOW })).toHaveFocus();
    expect(answered).toEqual([]);
  });

  it("Enter twice on the hand dismisses, and grants nothing", async () => {
    const answered: string[] = [];
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[]}
        away={[item(), item({ target: "qa" })]}
        onPress={() => {}}
        onAllowAway={() => answered.push("allow")}
        onDismissAway={(one) => answered.push(`dismiss ${one.target}`)}
      />,
    );
    hand().focus();

    await userEvent.keyboard("{Enter}");
    await waitFor(() =>
      expect(screen.getAllByRole("menuitem", { name: /^Dismiss:/ })[0]).toHaveFocus(),
    );
    await userEvent.keyboard("{Enter}");

    expect(answered).toEqual(["dismiss devops"]);
  });
});

describe("the list holds still while it is open", () => {
  /** A list the test changes from outside, as the core's event does. */
  function Changing({ first }: { first: readonly AwayItem[] }) {
    const [away, setAway] = useState(first);
    const [answered, setAnswered] = useState<string[]>([]);
    return (
      <>
        <NeedsYouMenu
          quiet={[]}
          items={[]}
          away={away}
          onPress={() => {}}
          onAllowAway={(one) => setAnswered((was) => [...was, `allow ${one.target}`])}
        />
        <button
          type="button"
          data-testid="core-says"
          onClick={(event) =>
            setAway(JSON.parse(event.currentTarget.dataset.next ?? "[]") as AwayItem[])
          }
        />
        <output>{answered.join(",")}</output>
      </>
    );
  }
  /** The core says the list anew, without the pointer or the keyboard going anywhere. */
  function coreSays(next: readonly AwayItem[]) {
    const say = screen.getByTestId("core-says");
    say.dataset.next = JSON.stringify(next);
    fireEvent.click(say);
  }

  it("draws no row a chat brought, moves none, and counts none up, until it is closed", async () => {
    const planner = item({ asking: "planner", target: "reviewer" });
    const steward = item();
    render(<Changing first={[planner, steward]} />);
    await userEvent.click(hand());
    await screen.findByRole("group", { name: /^planner wanted reviewer/ });
    const before = rows();
    expect(before).toEqual([
      "planner wanted reviewer while you were away",
      "steward wanted devops while you were away",
    ]);

    // A chat asks again and for another persona: a new row, another order, a higher count.
    coreSays([item({ target: "qa" }), { ...steward, times: 99 }, planner]);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: /^3 dispatches were refused/ })).toBeTruthy(),
    );
    expect(rows()).toEqual(before);
    expect(screen.getByRole("group", { name: /^steward wanted devops/ })).toHaveTextContent(
      "3 times",
    );
    // A press where the person aimed is the pair they read.
    await userEvent.click(
      screen.getByRole("menuitem", {
        name: /^Allow from now on: planner chats dispatch to reviewer/,
      }),
    );
    expect(screen.getByRole("status")).toHaveTextContent("allow reviewer");

    // Opened again, it is the core's list as it stands, in the core's order.
    await userEvent.click(screen.getByRole("button", { name: /^3 dispatches were refused/ }));
    await screen.findByRole("group", { name: /^steward wanted qa/ });
    expect(rows()).toEqual([
      "steward wanted qa while you were away",
      "steward wanted devops while you were away",
      "planner wanted reviewer while you were away",
    ]);
  });

  it("marks a row that went and keeps it in its place, with its answers off", async () => {
    const planner = item({ asking: "planner", target: "reviewer" });
    const answered: string[] = [];
    function Going() {
      const [away, setAway] = useState<readonly AwayItem[]>([item(), planner]);
      return (
        <>
          <NeedsYouMenu
            quiet={[]}
            items={[]}
            away={away}
            onPress={() => {}}
            onAllowAway={(one) => answered.push(`allow ${one.target}`)}
            onDismissAway={(one) => answered.push(`dismiss ${one.target}`)}
            onNeverAway={(one) => answered.push(`never ${one.target}`)}
          />
          <button type="button" data-testid="went" onClick={() => setAway([planner])} />
        </>
      );
    }
    render(<Going />);
    await userEvent.click(hand());
    await screen.findByRole("group", { name: /^steward wanted devops/ });
    const before = rows();

    fireEvent.click(screen.getByTestId("went"));

    const gone = await screen.findByRole("group", { name: /^steward wanted devops/ });
    await waitFor(() => expect(gone).toHaveTextContent(GONE));
    expect(rows()).toEqual(before);
    expect(gone).not.toHaveTextContent(ALLOWS);
    for (const answer of within(gone).getAllByRole("menuitem")) {
      expect(answer).toHaveAttribute("aria-disabled", "true");
      await userEvent.click(answer);
    }
    expect(answered).toEqual([]);
    // The row under it is where it was, and answers.
    await userEvent.click(
      screen.getByRole("menuitem", { name: /^Dismiss: planner wanted reviewer/ }),
    );
    expect(answered).toEqual(["dismiss reviewer"]);
  });
});

/** The list as the window holds it for one project, with its answers and what was said. */
function Held() {
  const away = useAwayRefusals([PLANE]);
  const [said, setSaid] = useState<string[]>([]);
  const answer = (how: "allow" | "dismiss" | "never") => (one: AwayItem) =>
    void away[how](one.plane, one).then((answered) => {
      if (answered)
        setSaid((was) => [...was, `${answered.refused ? "refused" : "said"}: ${answered.words}`]);
    });
  return (
    <>
      <NeedsYouMenu
        quiet={[]}
        items={[]}
        away={(away.held[PLANE] ?? []).map((one) => ({ ...one, plane: PLANE, project: "charter" }))}
        onPress={() => {}}
        onAllowAway={answer("allow")}
        onDismissAway={answer("dismiss")}
        onNeverAway={answer("never")}
        onLook={away.read}
      />
      <output>{said.join("\n")}</output>
    </>
  );
}

describe("the list, read from the core and answered through it", () => {
  const found = () => screen.findByRole("button", { name: /refused while you were away$/ });

  it("keeps the core's order, which no later refusal moves", async () => {
    mockIPC((cmd) =>
      cmd === "dispatch_away"
        ? [refusal({ latest: NOW - 9000 }), refusal({ target: "qa", latest: NOW - 10 })]
        : undefined,
    );
    render(<Held />);

    await userEvent.click(await found());

    await screen.findByRole("group", { name: /^steward wanted qa/ });
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

    await userEvent.click(await found());
    await userEvent.click(await screen.findByRole("menuitem", { name: ALLOW }));

    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent("said: Allowed for me on this machine."),
    );
    expect(asked.filter((one) => one.cmd === "allow_dispatch_away").map((one) => one.args)).toEqual(
      [{ plane: PLANE, asking: "steward", target: "devops", workspace: "ide" }],
    );
    // No level and no wildcard is the window's to send.
    expect(asked.some((one) => /allow_dispatch$|allow_dispatch_to_any/.test(one.cmd))).toBe(false);
    expect(
      await screen.findByRole("button", { name: "1 dispatch was refused while you were away" }),
    ).toBeInTheDocument();
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

    await userEvent.click(await found());
    await userEvent.click(await screen.findByRole("menuitem", { name: /^Never for this pair:/ }));

    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        "said: No steward chat dispatches to devops on this machine.",
      ),
    );
    expect(asked.filter((one) => one.cmd === "never_dispatch_away").map((one) => one.args)).toEqual(
      [{ plane: PLANE, asking: "steward", target: "devops", workspace: "ide" }],
    );
    expect(asked.some((one) => one.cmd === "allow_dispatch_away")).toBe(false);
    await waitFor(() => expect(screen.queryByRole("button")).toBeNull());
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

    await userEvent.click(await found());
    await userEvent.click(
      await screen.findByRole("menuitem", { name: /^Dismiss: steward wanted devops/ }),
    );

    await waitFor(() => expect(screen.queryByRole("button")).toBeNull());
    expect(asked).toContain("dismiss_dispatch_away");
    expect(asked).not.toContain("allow_dispatch_away");
    expect(screen.getByRole("status")).toHaveTextContent("");
  });

  it("drops a pair you said never to by the time you look, without a word", async () => {
    let listed = [refusal(), refusal({ target: "qa" })];
    mockIPC((cmd) => (cmd === "dispatch_away" ? listed : undefined));
    render(<Held />);
    expect(
      await screen.findByRole("button", { name: "2 dispatches were refused while you were away" }),
    ).toBeInTheDocument();

    // Never for steward to devops, said in Settings since: the core no longer lists it. The
    // list is read again as the pointer comes to the hand, before it is opened.
    listed = [refusal({ target: "qa" })];
    await userEvent.hover(hand());
    const one = await screen.findByRole("button", {
      name: "1 dispatch was refused while you were away",
    });
    await userEvent.click(one);

    await screen.findByRole("group", { name: /^steward wanted qa/ });
    expect(screen.queryByRole("group", { name: /^steward wanted devops/ })).toBeNull();
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

    await userEvent.click(await found());
    await userEvent.click(await screen.findByRole("menuitem", { name: ALLOW }));

    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(`refused: ${never}`));
    await waitFor(() => expect(screen.queryByRole("button")).toBeNull());
  });
});
