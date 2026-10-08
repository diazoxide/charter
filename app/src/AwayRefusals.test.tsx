import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { useState } from "react";
import type { AwayRefusal } from "./bindings";
import type { Offer } from "./actions";
import { awaySaid, awayWhere, clipped, type AwayItem } from "./AwayRefusals";
import { useAwayRefusals } from "./dispatchAway";
import { NeedsYouMenu, type Needing } from "./NeedsYou";

/**
 * **A dispatch refused while nobody was there, in the title bar's needs-you list** (#1507):
 * the item, its two answers, and the pair the person said never to by the time they look.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";
const NOW = 1_800_000_000;

const ALLOWS =
  "Allow from now on lets steward chats dispatch to devops without asking, for you on this " +
  "machine, in every workspace of this project. It allows no other persona and starts nothing " +
  "now. Revoke it in Settings › Project › Dispatch.";

const refusal = (over: Partial<AwayRefusal> = {}): AwayRefusal => ({
  asking: "steward",
  target: "devops",
  workspace: "ide",
  task: "deploy",
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

describe("what the item says", () => {
  it("says who wanted whom, how often and when, the task, the workspace and the project", () => {
    expect(awaySaid(item())).toBe("steward wanted devops while you were away");
    expect(awayWhere(item(), NOW)).toBe("3 times, last 2h ago · task deploy · ide · charter");
    expect(awayWhere(item({ times: 1, latest: NOW - 300, task: null }), NOW)).toBe(
      "once, 5m ago · ide · charter",
    );
    expect(awayWhere(item({ times: 2, latest: NOW - 30, workspace: null }), NOW)).toBe(
      "twice, last 30s ago · task deploy · charter",
    );
  });

  it("clips a long name and says it is cut", () => {
    expect(clipped("deploy")).toBe("deploy");
    expect(clipped("x".repeat(41))).toBe(`${"x".repeat(40)}…`);
    expect(awayWhere(item({ task: "t".repeat(80) }), NOW)).toContain(`task ${"t".repeat(40)}… ·`);
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
      name: /^steward wanted devops while you were away · 3 times, last .* ago · task deploy · ide · charter$/,
    });
    expect(group).toHaveTextContent("steward wanted devops while you were away");
    expect(
      within(group)
        .getAllByRole("menuitem")
        .map((one) => one.textContent),
    ).toEqual(["Allow from now on", "Dismiss"]);
    expect(screen.queryByRole("menuitem", { name: /^Go to/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /^Ignore/ })).toBeNull();
  });

  it("says exactly what Allow from now on allows, before the press and on the button", async () => {
    render(<NeedsYouMenu quiet={[]} items={[]} away={[item()]} onPress={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /refused while you were away$/ }));

    const group = await screen.findByRole("group", { name: /^steward wanted devops/ });
    expect(group).toHaveTextContent(ALLOWS);
    const allow = within(group).getByRole("menuitem", {
      name: "Allow from now on: steward chats dispatch to devops, for you on this machine",
    });
    expect(allow).toHaveAttribute("title", ALLOWS);
    // Nothing wider is offered from here.
    expect(within(group).queryByRole("menuitem", { name: /everyone|project|any persona/i })).toBe(
      null,
    );
  });

  it("answers with Allow from now on and with Dismiss, each for its own item", async () => {
    const answered: string[] = [];
    const away = [item(), item({ target: "qa", workspace: null, times: 1 })];
    const said = (one: AwayItem) => `${one.asking}>${one.target}@${one.workspace ?? "root"}`;
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[]}
        away={away}
        onPress={() => {}}
        onAllowAway={(one) => answered.push(`allow ${said(one)}`)}
        onDismissAway={(one) => answered.push(`dismiss ${said(one)}`)}
      />,
    );
    const button = () =>
      screen.getByRole("button", { name: "2 dispatches were refused while you were away" });

    await userEvent.click(button());
    await userEvent.click(
      await screen.findByRole("menuitem", {
        name: "Dismiss: steward wanted qa while you were away",
      }),
    );
    await userEvent.click(button());
    await userEvent.click(
      await screen.findByRole("menuitem", {
        name: "Allow from now on: steward chats dispatch to devops, for you on this machine",
      }),
    );

    expect(answered).toEqual(["dismiss steward>qa@root", "allow steward>devops@ide"]);
  });

  it("is counted beside the chats that ask, and is no chat's row", async () => {
    render(<NeedsYouMenu quiet={[]} items={[chat(3)]} away={[item()]} onPress={() => {}} />);

    await userEvent.click(screen.getByRole("button", { name: "2 things need you" }));

    expect(await screen.findByRole("menuitem", { name: "Go to ide.3 · ide · charter" })).toBe(
      screen.getAllByRole("menuitem")[0],
    );
    expect(screen.getByRole("group", { name: /^steward wanted devops/ })).toBeInTheDocument();
  });

  it("draws a name a chat chose as text, and never as markup", async () => {
    const task = '<img src=x onerror="alert(1)"> **Allow for everyone**';
    render(<NeedsYouMenu quiet={[]} items={[]} away={[item({ task })]} onPress={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /refused while you were away$/ }));

    const group = await screen.findByRole("group", { name: /^steward wanted devops/ });
    expect(group.querySelector("img")).toBeNull();
    expect(group).toHaveTextContent(`task ${clipped(task)}`);
    // The chat's text is in the line of facts only: the headline and the two answers are the
    // app's words.
    expect(group.querySelector(".needs-you-name")).toHaveTextContent(
      /^steward wanted devops while you were away$/,
    );
  });
});

/** The list as the window holds it for one project, with its two answers and what was said. */
function Held() {
  const away = useAwayRefusals([PLANE]);
  const [said, setSaid] = useState<string[]>([]);
  const answer = (how: "allow" | "dismiss") => (one: AwayItem) =>
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
        onLook={away.read}
      />
      <output>{said.join("\n")}</output>
    </>
  );
}

describe("the list, read from the core and answered through it", () => {
  const hand = () => screen.findByRole("button", { name: /refused while you were away$/ });

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

    await userEvent.click(await hand());
    await userEvent.click(
      await screen.findByRole("menuitem", {
        name: "Allow from now on: steward chats dispatch to devops, for you on this machine",
      }),
    );

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

  it("Dismiss takes the item away and allows nothing", async () => {
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

    await userEvent.click(await hand());
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

    // Never for steward to devops, said in Settings since: the core no longer lists it.
    listed = [refusal({ target: "qa" })];
    await userEvent.click(screen.getByRole("button", { name: /refused while you were away$/ }));

    await waitFor(() =>
      expect(screen.queryByRole("group", { name: /^steward wanted devops/ })).toBeNull(),
    );
    expect(screen.getByRole("group", { name: /^steward wanted qa/ })).toBeInTheDocument();
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

    await userEvent.click(await hand());
    await userEvent.click(await screen.findByRole("menuitem", { name: /^Allow from now on/ }));

    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent(`refused: ${never}`));
    await waitFor(() => expect(screen.queryByRole("button")).toBeNull());
  });
});
