import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Offer } from "./actions";

import { NeedsYouMenu, type Needing } from "./NeedsYou";

afterEach(cleanup);

/** One item as a project reports it: its rows are the catalogue's, as every surface's are. */
function needing(plane: string, session: number, workspace: string, project: string): Needing {
  const name = `${workspace}.${session}`;
  return {
    plane,
    session,
    name,
    workspace,
    project,
    go: {
      id: `needs.show:${session}`,
      title: `Show ${name}, which needs you`,
      available: true,
      reason: "",
      does: { verb: "showChat", session },
      name,
    },
    ignore: {
      id: `needs.ignore:${session}`,
      title: `Ignore ${name} until it asks again`,
      available: true,
      reason: "",
      does: { verb: "ignoreNeedsYou", session },
      name,
    },
  };
}

describe("the title bar's needs-you button (charter-app#249)", () => {
  it("draws nothing when nothing needs you", () => {
    const { container } = render(<NeedsYouMenu items={[]} onPress={() => {}} />);

    expect(screen.queryByRole("button")).toBeNull();
    expect(container).toHaveTextContent("");
  });

  it("says how many chats need you, in the count and in words", () => {
    render(
      <NeedsYouMenu
        items={[needing("/a", 1, "ide", "charter"), needing("/b", 2, "easydmarc", "devops")]}

        onPress={() => {}}
      />,
    );

    const button = screen.getByRole("button", { name: "2 chats need you" });
    expect(button).toHaveTextContent("2");
    expect(button).toHaveAttribute("tabindex", "0");
  });

  /** Two chats in two projects, as the operator's own preview has them, and what was pressed. */
  function two() {
    const pressed: string[] = [];
    render(
      <NeedsYouMenu
        items={[needing("/a", 1, "ide", "steward"), needing("/b", 2, "easydmarc", "devops")]}
        onPress={(plane: string, offer: Offer) => pressed.push(`${plane} ${offer.id}`)}
      />,
    );
    return pressed;
  }
  const button = () => screen.getByRole("button", { name: /chats? needs? you$/ });

  it("lists every chat asking, by name, then workspace and project", async () => {
    two();

    await userEvent.click(button());

    const menu = await screen.findByRole("menu");
    const items = within(menu).getAllByRole("menuitem");
    expect(items.map((item) => item.getAttribute("aria-label"))).toEqual([
      "Go to ide.1 · ide · steward",
      "Go to easydmarc.2 · easydmarc · devops",
    ]);
    expect(items[0]).toHaveTextContent(/ide\.1.*ide · steward.*Go/);
  });

  it("goes to a chat in its own project", async () => {
    const pressed = two();
    await userEvent.click(button());

    await userEvent.click(await screen.findByRole("menuitem", { name: /^Go to easydmarc\.2/ }));

    expect(pressed).toEqual(["/b needs.show:2"]);
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  });

  it("ignores a chat with its ✕, and leaves the list open", async () => {
    const pressed = two();
    await userEvent.click(button());

    await userEvent.click(
      await screen.findByRole("button", { name: "Ignore ide.1 until it asks again" }),
    );

    expect(pressed).toEqual(["/a needs.ignore:1"]);
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("opens from the keyboard on its first chat, and the arrows move down the list", async () => {
    two();
    button().focus();

    await userEvent.keyboard("{Enter}");

    const first = await screen.findByRole("menuitem", { name: /^Go to ide\.1/ });
    await waitFor(() => expect(first).toHaveFocus());
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: /^Go to easydmarc\.2/ })).toHaveFocus();
  });

  it("closes on Escape and gives the keyboard back to the button", async () => {
    two();
    button().focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(screen.getAllByRole("menuitem")[0]).toHaveFocus());

    await userEvent.keyboard("{Escape}");

    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    await waitFor(() => expect(button()).toHaveFocus());
  });

  it("ignores the focused chat on Delete, and leaves the keyboard on the next one", async () => {
    const pressed = two();
    button().focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(screen.getAllByRole("menuitem")[0]).toHaveFocus());

    await userEvent.keyboard("{Delete}");

    expect(pressed).toEqual(["/a needs.ignore:1"]);
    await waitFor(() =>
      expect(screen.getByRole("menuitem", { name: /^Go to easydmarc\.2/ })).toHaveFocus(),
    );
  });

  it("keeps a chat it cannot go to in the list, saying so, so Delete still reaches it", async () => {
    const pressed: string[] = [];
    const stray = needing("/a", 1, "ide", "steward");
    stray.go = {
      id: "needs.show:1",
      title: "Show ide.1, which needs you",
      available: false,
      reason: "That chat has no tab in this window.",
      does: { verb: "nothing" },
      name: "ide.1",
    };
    render(
      <NeedsYouMenu
        items={[stray]}
        onPress={(plane, offer) => pressed.push(`${plane} ${offer.id}`)}
      />,
    );
    await userEvent.click(button());

    const item = await screen.findByRole("menuitem", { name: /^Go to ide\.1/ });
    expect(item).toHaveAttribute("aria-disabled", "true");
    expect(item).toHaveAttribute("title", "That chat has no tab in this window.");
    await userEvent.click(item);
    expect(pressed).toEqual([]);
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("leaves the keyboard on the one before when the last chat is ignored", async () => {
    two();
    button().focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(screen.getAllByRole("menuitem")[0]).toHaveFocus());
    await userEvent.keyboard("{ArrowDown}");

    await userEvent.keyboard("{Delete}");

    await waitFor(() =>
      expect(screen.getByRole("menuitem", { name: /^Go to ide\.1/ })).toHaveFocus(),
    );
  });

  it("leaves a chord alone", async () => {
    const pressed = two();
    button().focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(screen.getAllByRole("menuitem")[0]).toHaveFocus());

    await userEvent.keyboard("{Shift>}{Delete}{/Shift}");

    expect(pressed).toEqual([]);
  });

  it("hands the keyboard to the next control in the bar when the last chat leaves", async () => {
    // The button goes with the last chat, and focus on an element that goes is focus on the
    // page, where the next key does nothing.
    const one = [needing("/a", 1, "ide", "steward")];
    const bar = (items: Needing[]) => (
      <header>
        <NeedsYouMenu items={items} onPress={() => {}} />
        <button type="button" tabIndex={0}>
          About
        </button>
      </header>
    );
    const { rerender } = render(bar(one));
    button().focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(screen.getByRole("menuitem")).toHaveFocus());
    await userEvent.keyboard("{Delete}");

    rerender(bar([]));

    await waitFor(() => expect(screen.getByRole("button", { name: "About" })).toHaveFocus());
  });

  it("does not spring open again when a chat asks after the list emptied", async () => {
    const one = [needing("/a", 1, "ide", "steward")];
    const { rerender } = render(<NeedsYouMenu items={one} onPress={() => {}} />);
    await userEvent.click(button());
    await screen.findByRole("menu");

    rerender(<NeedsYouMenu items={[]} onPress={() => {}} />);
    rerender(<NeedsYouMenu items={one} onPress={() => {}} />);

    expect(button()).toBeInTheDocument();
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

/**
 * **The count's colour is only ever on a pair the contrast suite measures.**
 *
 * `needs-you.base` is `#b85050` in charter-dark, which is 3.64:1 on `surface.base` — under AA
 * for words. So the button's own colour is `text.primary`, and the number is
 * `needs-you.text` FILLED with `needs-you.base`, which is the pair `contrast.test.ts` holds at
 * 4.5:1. jsdom computes no colour, so this reads the rules.
 */
describe("the needs-you count's colours", () => {
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
    /\/\*[\s\S]*?\*\//g,
    "",
  );
  const rule = (selector: string) =>
    new RegExp(`(?:^|\\})\\s*${selector.replace(/[.]/g, "\\.")}\\s*\\{([^}]*)\\}`).exec(css)?.[1] ??
    "";

  it("never writes the button's words in needs-you.base", () => {
    expect(rule(".needs-you-button")).toMatch(/(?:^|[;\s])color:\s*var\(--text-primary\)/);
    expect(rule(".needs-you-button")).not.toMatch(/(?:^|[;\s])color:\s*var\(--needs-you-base\)/);
  });

  it("draws no coloured border on the button or the list (charter-app#249)", () => {
    expect(rule(".needs-you-button")).toMatch(/border:\s*none/);
    for (const selector of [".needs-you-button", ".needs-you-go", ".needs-you-row"])
      expect(rule(selector)).not.toMatch(/border[a-z-]*:[^;]*var\(--(?!border-subtle)/);
  });

  it("fills the number with needs-you.base under needs-you.text, the measured pair", () => {
    expect(rule(".needs-you-number")).toMatch(/background:\s*var\(--needs-you-base\)/);
    expect(rule(".needs-you-number")).toMatch(/(?:^|[;\s])color:\s*var\(--needs-you-text\)/);
  });
});
