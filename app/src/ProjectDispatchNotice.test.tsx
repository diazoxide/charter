import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ProjectDispatchNotice } from "./ProjectDispatchNotice";
import type { DispatchArrival, DispatchArrived, DispatchGone } from "./bindings";

/**
 * The Notice that says a teammate's dispatch grant arrived (#1506): what the project's
 * settings now let which persona's chats do, with Accept and Not on my machine for all that is
 * listed, and what the project took away, said once.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";
const LABEL = "The project's dispatch grants changed";
const GONE = "The project took dispatch grants away";

const arrived = (asking: string, target: string, more: Partial<DispatchArrived> = {}) => ({
  id: `${asking} -> ${target}`,
  asking,
  target,
  any: target === "*",
  undefined: null,
  again: false,
  ...more,
});

const gone = (asking: string, target: string): DispatchGone => ({
  id: `${asking} -> ${target}`,
  asking,
  target,
  any: target === "*",
});

const QUIET: DispatchArrival = { waiting: [], gone: [] };

/** A core where `first` waits; an answer is recorded and answered with `after`. */
function core(first: DispatchArrival, after: { said: string | null; arrival: DispatchArrival }) {
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

describe("the Notice that a teammate's dispatch grant arrived", () => {
  it("says one pair in a sentence, and that nothing is in force until it is accepted", async () => {
    core({ waiting: [arrived("steward", "devops")], gone: [] }, { said: null, arrival: QUIET });
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(notice).toHaveTextContent("The project now lets steward dispatch to devops.");
    expect(notice).toHaveTextContent("None of it is in force on this machine until you accept it.");
    expect(
      within(notice)
        .getAllByRole("button")
        .map((one) => one.textContent),
    ).toEqual(["Accept", "Not on my machine", "Decide each in Settings", "Dismiss"]);
  });

  it("lists several pairs in one Notice, by the persona that may dispatch", async () => {
    core(
      {
        waiting: [arrived("steward", "devops"), arrived("steward", "qa"), arrived("qa", "devops")],
        gone: [],
      },
      { said: null, arrival: QUIET },
    );
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(screen.getAllByRole("status", { name: LABEL })).toHaveLength(1);
    expect(notice).toHaveTextContent(
      "The project now lets steward dispatch to devops and qa; qa dispatch to devops.",
    );
    expect(within(notice).getByRole("button", { name: "Accept all 3" })).toBeInTheDocument();
  });

  it("says any persona in its own words, before the pairs", async () => {
    core(
      { waiting: [arrived("steward", "*"), arrived("qa", "devops")], gone: [] },
      { said: null, arrival: QUIET },
    );
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(notice).toHaveTextContent(
      "The project now lets steward dispatch to any persona: every persona of this project, including ones added later. The project now lets qa dispatch to devops.",
    );
  });

  it("accepts everything it listed, by what was shown, and is gone", async () => {
    const waiting = [arrived("steward", "*"), arrived("steward", "devops")];
    const asked = core({ waiting, gone: [] }, { said: null, arrival: QUIET });
    show();

    await userEvent.setup().click(await screen.findByRole("button", { name: "Accept all 2" }));

    await waitFor(() =>
      expect(screen.queryByRole("status", { name: LABEL })).not.toBeInTheDocument(),
    );
    expect(sent(asked, "answer_dispatch_arrival")).toMatchObject([
      { plane: PLANE, accepted: true, shown: ["steward -> *", "steward -> devops"] },
    ]);
  });

  it("declines everything it listed on Not on my machine", async () => {
    const asked = core(
      { waiting: [arrived("steward", "devops"), arrived("qa", "devops")], gone: [] },
      { said: null, arrival: QUIET },
    );
    show();

    await userEvent.setup().click(await screen.findByRole("button", { name: "Not on my machine" }));

    await waitFor(() =>
      expect(screen.queryByRole("status", { name: LABEL })).not.toBeInTheDocument(),
    );
    expect(sent(asked, "answer_dispatch_arrival")).toMatchObject([
      { plane: PLANE, accepted: false, shown: ["steward -> devops", "qa -> devops"] },
    ]);
  });

  it("is put away by Dismiss without answering anything", async () => {
    const asked = core(
      { waiting: [arrived("steward", "devops")], gone: [] },
      { said: null, arrival: QUIET },
    );
    show();

    await userEvent.setup().click(await screen.findByRole("button", { name: "Dismiss" }));

    expect(screen.queryByRole("status", { name: LABEL })).not.toBeInTheDocument();
    expect(sent(asked, "answer_dispatch_arrival")).toEqual([]);
    // Nothing but the read was ever sent to the core.
    expect(asked.map((one) => one.cmd).filter((cmd) => !cmd.startsWith("plugin:"))).toEqual([
      "dispatch_arrival",
    ]);
  });

  it("opens Settings to decide each, and answers nothing", async () => {
    const asked = core(
      { waiting: [arrived("steward", "devops")], gone: [] },
      { said: null, arrival: QUIET },
    );
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
    core(
      { waiting: [arrived("steward", "devops")], gone: [] },
      { said, arrival: { waiting: [arrived("steward", "prod")], gone: [] } },
    );
    show();

    await userEvent.setup().click(await screen.findByRole("button", { name: "Accept" }));

    const notice = await screen.findByRole("status", { name: LABEL });
    await waitFor(() => expect(notice).toHaveTextContent(said));
    expect(notice).toHaveTextContent("The project now lets steward dispatch to prod.");
    expect(notice).not.toHaveTextContent("steward dispatch to devops");
  });

  it("says a grant names a persona this project does not define, and does not accept it", async () => {
    const asked = core(
      {
        waiting: [
          arrived("steward", "devops"),
          arrived("steward", "ghost", { undefined: "ghost" }),
        ],
        gone: [],
      },
      { said: null, arrival: QUIET },
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
    core(
      { waiting: [arrived("steward", "ghost", { undefined: "ghost" })], gone: [] },
      { said: null, arrival: QUIET },
    );
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(within(notice).queryByRole("button", { name: /^Accept/ })).toBeNull();
    expect(notice).not.toHaveTextContent("until you accept it");
    expect(within(notice).getByRole("button", { name: "Not on my machine" })).toBeInTheDocument();
  });

  it("says a grant is asked again when the project took it away and put it back", async () => {
    core(
      { waiting: [arrived("steward", "devops", { again: true })], gone: [] },
      { said: null, arrival: QUIET },
    );
    show();

    expect(await screen.findByRole("status", { name: LABEL })).toHaveTextContent(
      "You accepted steward to devops before. The project's settings were without it for a time since, so it waits for your yes again.",
    );
  });

  it("sums a flood of pairs, keeps any persona apart, and lists each pair under the line", async () => {
    const targets = ["a", "b", "c", "d", "e", "f", "g", "h"];
    core(
      {
        waiting: [arrived("qa", "*"), ...targets.map((target) => arrived("steward", target))],
        gone: [],
      },
      { said: null, arrival: QUIET },
    );
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(notice).toHaveTextContent("The project now lets qa dispatch to any persona");
    expect(notice).toHaveTextContent(
      "The project now lets steward dispatch to others: 8 pairs in all, each listed below.",
    );
    expect(screen.getByText("Show all 8 pairs")).toBeInTheDocument();
    expect(screen.getAllByRole("listitem").map((one) => one.textContent)).toEqual(
      targets.map((target) => `steward to ${target}`),
    );
  });

  it("draws a name as text, never as markup", async () => {
    core(
      { waiting: [arrived("steward", "<img src=x>", { undefined: "<img src=x>" })], gone: [] },
      { said: null, arrival: QUIET },
    );
    show();

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(notice).toHaveTextContent("steward dispatch to <img src=x>");
    expect(notice.querySelector("img")).toBeNull();
  });

  it("says once what the project took away, and asks nothing", async () => {
    const asked = core(
      { waiting: [], gone: [gone("steward", "billing"), gone("qa", "*")] },
      { said: null, arrival: QUIET },
    );
    show();

    const notice = await screen.findByRole("status", { name: GONE });
    expect(notice).toHaveTextContent(
      "The project no longer lets steward to billing, qa to any persona.",
    );
    expect(notice).toHaveTextContent("There is nothing to answer.");
    expect(
      within(notice)
        .getAllByRole("button")
        .map((one) => one.textContent),
    ).toEqual(["Dismiss"]);
    expect(screen.queryByRole("status", { name: LABEL })).not.toBeInTheDocument();

    await userEvent.setup().click(within(notice).getByRole("button", { name: "Dismiss" }));

    await waitFor(() =>
      expect(screen.queryByRole("status", { name: GONE })).not.toBeInTheDocument(),
    );
    expect(sent(asked, "dispatch_gone_told")).toMatchObject([
      { plane: PLANE, shown: ["steward -> billing", "qa -> *"] },
    ]);
  });

  it("says nothing when nothing waits", async () => {
    const asked = core(QUIET, { said: null, arrival: QUIET });
    show();

    await waitFor(() => expect(asked.map((one) => one.cmd)).toContain("dispatch_arrival"));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});
