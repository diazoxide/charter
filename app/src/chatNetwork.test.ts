import { describe, expect, it } from "vitest";
import {
  chatNetworkSession,
  chatNetworkTitle,
  chatNetworkView,
  isChatNetwork,
} from "./chatNetwork";
import { contentsOf, noTabs, openTab, openView, replaceSession } from "./tabs";

describe("a chat's Network tab (#1662)", () => {
  it("is keyed by the chat's number and titled by its name", () => {
    const view = chatNetworkView(7);
    expect(isChatNetwork(view)).toBe(true);
    expect(chatNetworkSession(view)).toBe(7);
    expect(chatNetworkSession({ ...view, key: "../x" })).toBeUndefined();
    expect(chatNetworkTitle("fix the build")).toBe("Network · fix the build");
  });

  it("goes with its chat to its new number when the chat is restarted", () => {
    let tabs = openTab(noTabs(), 3, "steward 3");
    tabs = openView(tabs, chatNetworkView(3), "Network · steward 3", "alpha");
    tabs = openView(tabs, chatNetworkView(8), "Network · lint", "alpha");

    const now = replaceSession(tabs, 3, 12);

    const shown = now.order.flatMap((id) =>
      contentsOf(now, id).map(({ content }) =>
        content.kind === "session" ? `chat ${content.session}` : content.view.key,
      ),
    );
    expect(shown).toEqual(["chat 12", "12", "8"]);
  });
});
