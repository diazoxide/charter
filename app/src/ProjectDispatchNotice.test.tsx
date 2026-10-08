import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ProjectDispatchNotice } from "./ProjectDispatchNotice";
import type { DispatchArrival, DispatchArrived, DispatchGone } from "./bindings";

/**
 * The Notice that says a teammate's dispatch grant arrived (#1506): what the project's
 * settings now let which persona's chats do, Accept for named pairs and Not on my machine for
 * all that is listed, "any persona" told and never accepted, and what the project took away,
 * said once.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";
const LABEL = "The project's dispatch grants changed";
const GONE = "The project took dispatch grants away";
const UNREAD = "The project's dispatch grants cannot be checked";

const arrived = (
  asking: string,
  target: string,
  more: Partial<DispatchArrived> = {},
): DispatchArrived => ({
  id: `${asking} -> ${target}`,
  asking,
  target,
  any: target === "*",
  undefined: null,
  again: null,
  works_with: null,
  ...more,
});

const gone = (asking: string, target: string): DispatchGone => ({
  id: `${asking} -> ${target}`,
  asking,
  target,
  any: target === "*",
});

const waits = (...waiting: DispatchArrived[]): DispatchArrival => ({
  waiting,
  gone: [],
  unread: false,
});

const QUIET: DispatchArrival = waits();
const DONE = { said: null, arrival: QUIET };

/** A core where `first` waits; an answer is recorded and answered with `after`. */
function core(
  first: DispatchArrival,
  after: { said: string | null; arrival: DispatchArrival } = DONE,
) {
  const asked: { cmd: string; args: unknown }[] = [];
  let now = first;
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "dispatch_arrival") return now;
    if (cmd === "answer_dispatch_arrival") {
      now = after.arrival;
      return after;
    }
    if (cmd === "dispatch_gone_told") {
      now = { ...now, gone: [] };
      return now;
    }
    return null;
  });
  return asked;
}

const sent = (asked: { cmd: string; args: unknown }[], cmd: string) =>
  asked.filter((one) => one.cmd === cmd).map((one) => one.args);

const show = (onReview: () => void = () => {}) =>
  render(<ProjectDispatchNotice plane={PLANE} onReview={onReview} />);

const buttons = (notice: HTMLElement) =>
  within(notice)
    .getAllByRole("button")
    .map((one) => one.textContent);

describe("the Notice that a teammate's dispatch grant arrived", () => {
  it("says one pair in a sentence, and that it is not in force until it is accepted", async () => {
    core(waits(arrived("steward", "devops")));
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(notice).toHaveTextContent("The project now lets steward dispatch to devops.");
    expect(notice).toHaveTextContent(
      "The pair is not in force on this machine until you accept it.",
    );
    expect(buttons(notice)).toEqual([
      "Accept",
      "Not on my machine",
      "Decide each in Settings",
      "Dismiss",
    ]);
  });

  it("lists several pairs in one Notice, by the persona that may dispatch", async () => {
    core(waits(arrived("steward", "devops"), arrived("steward", "qa"), arrived("qa", "devops")));
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(screen.getAllByRole("status", { name: LABEL })).toHaveLength(1);
    expect(notice).toHaveTextContent(
      "The project now lets steward dispatch to devops and qa; qa dispatch to devops.",
    );
    expect(within(notice).getByRole("button", { name: "Accept all 3" })).toBeInTheDocument();
  });

  it("tells of any persona in its own sentence, and Accept counts and sends the pairs only", async () => {
    const asked = core(waits(arrived("steward", "*"), arrived("qa", "devops")));
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(notice).toHaveTextContent(
      "The project now lets steward dispatch to any persona: every persona of this project, including ones added later. It is not in force on this machine, and is accepted only in Settings › Project › Dispatch. The project now lets qa dispatch to devops.",
    );
    // One pair: "Accept", never "Accept all 2".
    expect(buttons(notice)).toEqual([
      "Accept",
      "Not on my machine",
      "Decide each in Settings",
      "Dismiss",
    ]);

    await userEvent.setup().click(within(notice).getByRole("button", { name: "Accept" }));

    await waitFor(() => expect(sent(asked, "answer_dispatch_arrival")).toHaveLength(1));
    expect(sent(asked, "answer_dispatch_arrival")).toMatchObject([
      { plane: PLANE, accepted: true, shown: ["qa -> devops"] },
    ]);
  });

  it("offers no Accept for any persona alone: only Not on my machine and Dispatch settings", async () => {
    const asked = core(waits(arrived("steward", "*")));
    const onReview = vi.fn();
    show(onReview);

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(buttons(notice)).toEqual(["Not on my machine", "Open Dispatch settings", "Dismiss"]);
    expect(notice).not.toHaveTextContent("until you accept");

    await userEvent
      .setup()
      .click(within(notice).getByRole("button", { name: "Open Dispatch settings" }));
    expect(onReview).toHaveBeenCalledOnce();
    expect(sent(asked, "answer_dispatch_arrival")).toEqual([]);
  });

  it("declines everything it listed on Not on my machine, any persona included", async () => {
    const asked = core(waits(arrived("steward", "*"), arrived("qa", "devops")));
    show();

    await userEvent.setup().click(await screen.findByRole("button", { name: "Not on my machine" }));

    await waitFor(() =>
      expect(screen.queryByRole("status", { name: LABEL })).not.toBeInTheDocument(),
    );
    expect(sent(asked, "answer_dispatch_arrival")).toMatchObject([
      { plane: PLANE, accepted: false, shown: ["steward -> *", "qa -> devops"] },
    ]);
  });

  it("is put away by Dismiss without answering anything", async () => {
    const asked = core(waits(arrived("steward", "devops")));
    show();

    await userEvent.setup().click(await screen.findByRole("button", { name: "Dismiss" }));

    expect(screen.queryByRole("status", { name: LABEL })).not.toBeInTheDocument();
    // Nothing but the read was ever sent to the core.
    expect(asked.map((one) => one.cmd).filter((cmd) => !cmd.startsWith("plugin:"))).toEqual([
      "dispatch_arrival",
    ]);
  });

  it("opens Settings to decide each, and answers nothing", async () => {
    const asked = core(waits(arrived("steward", "devops")));
    const onReview = vi.fn();
    show(onReview);

    await userEvent
      .setup()
      .click(await screen.findByRole("button", { name: "Decide each in Settings" }));

    expect(onReview).toHaveBeenCalledOnce();
    expect(sent(asked, "answer_dispatch_arrival")).toEqual([]);
  });

  it("says where the list changed before the answer came, and shows the list as it is", async () => {
    const said =
      "Nothing was accepted: the project's dispatch grants changed after this was shown. This is what waits now.";
    core(waits(arrived("steward", "devops")), {
      said,
      arrival: waits(arrived("steward", "prod")),
    });
    show();

    await userEvent.setup().click(await screen.findByRole("button", { name: "Accept" }));

    const notice = await screen.findByRole("status", { name: LABEL });
    await waitFor(() => expect(notice).toHaveTextContent(said));
    expect(notice).toHaveTextContent("The project now lets steward dispatch to prod.");
    expect(notice).not.toHaveTextContent("steward dispatch to devops");
  });

  it("still says what an answer came to when nothing waits any more", async () => {
    const said =
      "Nothing was accepted: the project's dispatch grants changed after this was shown. This is what waits now.";
    core(waits(arrived("steward", "devops")), { said, arrival: QUIET });
    show();

    await userEvent.setup().click(await screen.findByRole("button", { name: "Accept" }));

    const notice = await screen.findByRole("status", { name: LABEL });
    await waitFor(() => expect(notice).toHaveTextContent(said));
    expect(buttons(notice)).toEqual(["Open Dispatch settings", "Dismiss"]);
    await userEvent.setup().click(within(notice).getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("status", { name: LABEL })).not.toBeInTheDocument();
  });

  it("says a grant names a persona this project does not define, and does not accept it", async () => {
    const asked = core(
      waits(arrived("steward", "devops"), arrived("steward", "ghost", { undefined: "ghost" })),
    );
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(notice).toHaveTextContent(
      "The project's settings also let steward dispatch to ghost, but this project does not define a persona named ghost, so that covers nothing and is not accepted.",
    );
    await userEvent.setup().click(within(notice).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(sent(asked, "answer_dispatch_arrival")).toHaveLength(1));
    expect(sent(asked, "answer_dispatch_arrival")).toMatchObject([
      { accepted: true, shown: ["steward -> devops"] },
    ]);
  });

  it("offers no Accept where nothing listed can be used", async () => {
    core(waits(arrived("steward", "ghost", { undefined: "ghost" })));
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(within(notice).queryByRole("button", { name: /^Accept/ })).toBeNull();
    expect(notice).not.toHaveTextContent("until you accept");
    expect(within(notice).getByRole("button", { name: "Not on my machine" })).toBeInTheDocument();
  });

  it("says why a grant accepted before waits again, a true sentence for each cause", async () => {
    core(
      waits(
        arrived("steward", "devops", { again: { why: "takenOut" } }),
        arrived("steward", "qa", { again: { why: "unread" } }),
        arrived("qa", "devops", { again: { why: "persona", name: "devops" } }),
      ),
    );
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(notice).toHaveTextContent(
      "You accepted steward to devops before. The project took it out of its settings and put it back, so it waits for a new yes.",
    );
    expect(notice).toHaveTextContent(
      "You accepted steward to qa before. purlis could not read the project's history since then and cannot tell whether it was there the whole time, so it waits for a new yes.",
    );
    expect(notice).toHaveTextContent(
      "You accepted qa to devops before. The persona devops was not in this project for a time and one of that name is there now, so it waits for a new yes.",
    );
    // Only the first says anyone took anything out.
    expect(notice.textContent?.match(/took (it|them) out/g)).toHaveLength(1);
  });

  it("keeps a re-ask apart from what is new: each has its own Accept", async () => {
    const asked = core(
      waits(
        arrived("steward", "devops", { again: { why: "takenOut" } }),
        arrived("steward", "qa", { again: { why: "takenOut" } }),
        arrived("qa", "prod"),
      ),
    );
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(buttons(notice)).toEqual([
      "Accept the 1 new",
      "Accept the 2 you accepted before",
      "Not on my machine",
      "Decide each in Settings",
      "Dismiss",
    ]);

    await userEvent
      .setup()
      .click(within(notice).getByRole("button", { name: "Accept the 2 you accepted before" }));

    await waitFor(() => expect(sent(asked, "answer_dispatch_arrival")).toHaveLength(1));
    expect(sent(asked, "answer_dispatch_arrival")).toMatchObject([
      { accepted: true, shown: ["steward -> devops", "steward -> qa"] },
    ]);
  });

  it("sums a flood of pairs, lists each under the line, and puts Accept first only once the list is opened", async () => {
    const targets = ["a", "b", "c", "d", "e", "f", "g", "h"];
    core(waits(arrived("qa", "*"), ...targets.map((target) => arrived("steward", target))));
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(notice).toHaveTextContent("The project now lets qa dispatch to any persona");
    expect(notice).toHaveTextContent(
      "The project now lets steward dispatch to others: 8 pairs in all, each listed below.",
    );
    expect(screen.getAllByRole("listitem").map((one) => one.textContent)).toEqual(
      targets.map((target) => `steward to ${target}`),
    );
    // Summed: what Accept would accept is not all in the line, so it does not lead.
    expect(buttons(notice).slice(0, 2)).toEqual(["Not on my machine", "Accept all 8"]);

    const list = screen.getByText("Show all 8 pairs").closest("details");
    expect(list).not.toBeNull();
    if (list !== null) {
      list.open = true;
      fireEvent(list, new Event("toggle"));
    }

    await waitFor(() =>
      expect(buttons(notice).slice(0, 2)).toEqual(["Accept all 8", "Not on my machine"]),
    );
  });

  it("says what each target works with, once a target, in the question's words", async () => {
    // #1502's words, from the core: what a yes to the pair reaches.
    const devops =
      "devops works with its own access: vault team; hosts k8s.internal.example:6443, and may itself dispatch to qa.";
    const billing = "billing works with its own access: no vault; no hosts beyond the project's.";
    core(
      waits(
        arrived("qa", "*"),
        arrived("steward", "devops", { works_with: devops }),
        arrived("qa", "devops", { works_with: devops }),
        arrived("steward", "billing", { works_with: billing }),
        arrived("steward", "ghost", { undefined: "ghost" }),
      ),
    );
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(notice).toHaveTextContent(`${devops} ${billing} `);
    // Once a target, however many pairs name it; and before what accepting does.
    const text = notice.textContent ?? "";
    expect(text.split(devops)).toHaveLength(2);
    expect(text.indexOf(billing)).toBeLessThan(text.indexOf("Accepting lets those chats"));
  });

  it("puts what the targets work with under the line with the pairs, where the pairs are summed", async () => {
    const targets = ["a", "b", "c", "d", "e", "f", "g"];
    const said = (target: string) =>
      `${target} works with its own access: no vault; hosts ${target}.example.`;
    core(
      waits(...targets.map((target) => arrived("steward", target, { works_with: said(target) }))),
    );
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    const list = screen.getByText("Show all 7 pairs").closest("details");
    expect(list).not.toBeNull();
    for (const target of targets) expect(list).toHaveTextContent(said(target));
    // The line itself names no pair, so it says none of their sentences either.
    expect(notice.querySelector("p")).not.toHaveTextContent("works with its own access");
  });

  it("draws a name as text, never as markup", async () => {
    core(waits(arrived("steward", "<img src=x>", { undefined: "<img src=x>" })));
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(notice).toHaveTextContent("steward dispatch to <img src=x>");
    expect(notice.querySelector("img")).toBeNull();
  });

  it("says once what the project took away, and asks nothing", async () => {
    const asked = core({
      waiting: [],
      gone: [gone("steward", "billing"), gone("qa", "*")],
      unread: false,
    });
    show();

    const notice = await screen.findByRole("status", { name: GONE });
    expect(notice).toHaveTextContent(
      "The project no longer lets steward dispatch to billing and qa dispatch to any persona.",
    );
    expect(notice).toHaveTextContent("There is nothing to answer.");
    expect(buttons(notice)).toEqual(["Dismiss"]);
    expect(screen.queryByRole("status", { name: LABEL })).not.toBeInTheDocument();

    await userEvent.setup().click(within(notice).getByRole("button", { name: "Dismiss" }));

    await waitFor(() =>
      expect(screen.queryByRole("status", { name: GONE })).not.toBeInTheDocument(),
    );
    expect(sent(asked, "dispatch_gone_told")).toMatchObject([
      { plane: PLANE, shown: ["steward -> billing", "qa -> *"] },
    ]);
  });

  it("says no accepted project grant counts while the project's history cannot be read", async () => {
    core({ waiting: [], gone: [], unread: true });
    show();

    const notice = await screen.findByRole("status", { name: UNREAD });
    expect(notice).toHaveTextContent(
      "Until it can, no dispatch grant of the project's that you accepted counts on this machine, and none can be accepted",
    );
    expect(buttons(notice)).toEqual(["Open Dispatch settings"]);
  });

  it("says nothing when nothing waits", async () => {
    const asked = core(QUIET);
    show();

    await waitFor(() => expect(asked.map((one) => one.cmd)).toContain("dispatch_arrival"));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});
