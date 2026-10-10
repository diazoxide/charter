import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Offer } from "./actions";

import { NeedsYouMenu, type Needing, type OtherAsk, type PermissionAsk } from "./NeedsYou";

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

describe("a chat a handed-off chat reported back to, in the title bar's list (charter-app#259)", () => {
  it("says who reported back, and its Go and Ignore are the chat that asked", async () => {
    const pressed: string[] = [];
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[{ ...needing("/a", 3, "ops", "charter"), reported: ["drop commons"] }]}
        onPress={(plane: string, offer: Offer) => pressed.push(`${plane} ${offer.id}`)}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "1 chat needs you" }));

    const go = await screen.findByRole("menuitem", {
      name: "Go to ops.3: drop commons reported back · ops · charter",
    });
    expect(go).toHaveTextContent(/drop commons reported back.*ops · charter.*Go/);
    await userEvent.click(screen.getByRole("button", { name: "Ignore ops.3 until it asks again" }));
    await userEvent.click(go);

    expect(pressed).toEqual(["/a needs.ignore:3", "/a needs.show:3"]);
  });

  it("names every chat that reported back, oldest first", async () => {
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[
          { ...needing("/a", 3, "ops", "charter"), reported: ["drop commons", "retry hooks"] },
        ]}
        onPress={() => {}}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "1 chat needs you" }));

    expect(
      await screen.findByRole("menuitem", { name: /drop commons, retry hooks reported back/ }),
    ).toBeInTheDocument();
  });
});

describe("the title bar's needs-you button (charter-app#249)", () => {
  it("draws nothing when nothing needs you", () => {
    const { container } = render(<NeedsYouMenu quiet={[]} items={[]} onPress={() => {}} />);

    expect(screen.queryByRole("button")).toBeNull();
    expect(container).toHaveTextContent("");
  });

  it("says how many chats need you, in the count and in words", () => {
    render(
      <NeedsYouMenu
        quiet={[]}
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
        quiet={[]}
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
        quiet={[]}
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
        <NeedsYouMenu quiet={[]} items={items} onPress={() => {}} />
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
    const { rerender } = render(<NeedsYouMenu quiet={[]} items={one} onPress={() => {}} />);
    await userEvent.click(button());
    await screen.findByRole("menu");

    rerender(<NeedsYouMenu quiet={[]} items={[]} onPress={() => {}} />);
    rerender(<NeedsYouMenu quiet={[]} items={one} onPress={() => {}} />);

    expect(button()).toBeInTheDocument();
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("the muted hand: a chat that cannot say it is waiting (charter-app#52, #249)", () => {
  const quiet = [{ name: "shell 2", project: "charter" }];

  it("shows a faint hand with no number when only a chat that cannot report is open", () => {
    render(<NeedsYouMenu items={[]} quiet={quiet} onPress={() => {}} />);

    const hand = screen.getByRole("button", {
      name: "Nothing has asked for you, but shell 2 can't tell purlis it's waiting",
    });
    expect(hand).toHaveClass("muted");
    expect(hand).toHaveAttribute("title", hand.getAttribute("aria-label"));
    expect(hand).toHaveTextContent(/^$/);
    expect(hand).toHaveAttribute("tabindex", "0");
  });

  it("counts several such chats in its name rather than listing them", () => {
    render(
      <NeedsYouMenu
        items={[]}
        quiet={[...quiet, { name: "codex 4", project: "ops" }]}
        onPress={() => {}}
      />,
    );

    expect(
      screen.getByRole("button", {
        name: "Nothing has asked for you, but 2 chats can't tell purlis they're waiting",
      }),
    ).toBeInTheDocument();
  });

  it("names each such chat, and its project, in the list it opens", async () => {
    render(
      <NeedsYouMenu
        items={[]}
        quiet={[...quiet, { name: "codex 4", project: "ops" }]}
        onPress={() => {}}
      />,
    );

    await userEvent.click(screen.getByRole("button"));

    const menu = await screen.findByRole("menu");
    expect(menu).toHaveTextContent("shell 2 · charter can't tell purlis it's waiting");
    expect(menu).toHaveTextContent("codex 4 · ops can't tell purlis it's waiting");
  });

  it("opens and closes from the keyboard the same way, and gives the keyboard back", async () => {
    render(<NeedsYouMenu items={[]} quiet={quiet} onPress={() => {}} />);
    const hand = screen.getByRole("button");
    hand.focus();

    await userEvent.keyboard("{Enter}");
    await screen.findByRole("menu");
    await userEvent.keyboard("{Escape}");

    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    await waitFor(() => expect(hand).toHaveFocus());
  });

  it("is the normal hand with the count once a chat really asks", () => {
    render(
      <NeedsYouMenu
        items={[needing("/a", 1, "ide", "steward")]}
        quiet={quiet}
        onPress={() => {}}
      />,
    );

    const hand = screen.getByRole("button", { name: "1 chat needs you" });
    expect(hand).not.toHaveClass("muted");
    expect(hand).toHaveTextContent("1");
  });

  it("is not there at all when nothing asked and every chat can say so", () => {
    render(<NeedsYouMenu items={[]} quiet={[]} onPress={() => {}} />);

    expect(screen.queryByRole("button")).toBeNull();
  });

  it("keeps the faint hand, with the keyboard on it, when the last request is ignored", async () => {
    const one = [needing("/a", 1, "ide", "steward")];
    const { rerender } = render(<NeedsYouMenu items={one} quiet={quiet} onPress={() => {}} />);
    screen.getByRole("button").focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(screen.getByRole("menuitem")).toHaveFocus());
    await userEvent.keyboard("{Delete}");

    rerender(<NeedsYouMenu items={[]} quiet={quiet} onPress={() => {}} />);

    await waitFor(() => expect(screen.getByRole("button")).toHaveClass("muted"));
    await waitFor(() => expect(screen.getByRole("button")).toHaveFocus());
  });

  it("does not bring back a chat it let go when another asks while it is still open", async () => {
    const one = [needing("/a", 1, "ide", "steward")];
    const { rerender } = render(<NeedsYouMenu items={one} quiet={quiet} onPress={() => {}} />);
    await userEvent.click(screen.getByRole("button"));
    await screen.findByRole("menu");

    rerender(<NeedsYouMenu items={[]} quiet={quiet} onPress={() => {}} />);
    expect(screen.queryByRole("menuitem", { name: /^Go to ide\.1/ })).toBeNull();
    rerender(
      <NeedsYouMenu
        items={[needing("/b", 2, "easydmarc", "devops")]}
        quiet={quiet}
        onPress={() => {}}
      />,
    );

    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /^Go to ide\.1/ })).toBeNull();
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
    new RegExp(
      `(?:^|\\})\\s*${selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*\\{([^}]*)\\}`,
    ).exec(css)?.[1] ?? "";

  it("never writes the button's words in needs-you.base", () => {
    expect(rule(".needs-you-button")).toMatch(/(?:^|[;\s])color:\s*var\(--text-primary\)/);
    expect(rule(".needs-you-button")).not.toMatch(/(?:^|[;\s])color:\s*var\(--needs-you-base\)/);
  });

  it("draws the faint hand in the muted text token, and no colour of its own", () => {
    expect(rule(".needs-you-button.muted,\n.needs-you-button.muted svg")).toMatch(
      /^\s*color:\s*var\(--text-muted\);\s*$/,
    );
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

describe("a chat's permission prompt, answered from the title bar's list (HP-6)", () => {
  /** Claude Code asking chat ops.3 to run `npm test`, with what it offers. */
  function permission(): PermissionAsk {
    return {
      plane: "/a",
      project: "charter",
      session: 3,
      name: "ops.3",
      ask: "01J9ZQ3V7K8M2N4P6R8T0V2X4Z",
      says: "Run npm test",
      options: [
        { id: "allow", label: "Allow", allows: true },
        { id: "suggestion:0", label: "Allow Bash(npm test) for this session", allows: true },
        { id: "deny", label: "Deny", allows: false },
      ],
    };
  }

  it("counts the ask, says what it asks, and answers it without going to the chat", async () => {
    const answered: string[] = [];
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[]}
        asks={[permission()]}
        onPress={() => {}}
        onAnswer={(ask, option) =>
          answered.push(`${ask.plane} ${ask.session} ${ask.ask} ${option}`)
        }
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "1 chat needs you" }));

    const group = await screen.findByRole("group", { name: "ops.3: Run npm test · charter" });
    expect(group).toHaveTextContent("Run npm test");
    await userEvent.click(
      within(group).getByRole("menuitem", { name: "Allow: ops.3, Run npm test" }),
    );

    expect(answered).toEqual(["/a 3 01J9ZQ3V7K8M2N4P6R8T0V2X4Z allow"]);
  });

  it("offers every option the harness offered, in its order and words", async () => {
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[]}
        asks={[permission()]}
        onPress={() => {}}
        onAnswer={() => {}}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "1 chat needs you" }));

    const group = await screen.findByRole("group", { name: /ops\.3: Run npm test/ });
    expect(
      within(group)
        .getAllByRole("menuitem")
        .map((item) => item.textContent),
    ).toEqual(["Allow", "Allow Bash(npm test) for this session", "Deny", "Open in its pane"]);
  });

  it("counts a chat asking and a permission prompt together", async () => {
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[needing("/a", 4, "ops", "charter")]}
        asks={[permission()]}
        onPress={() => {}}
        onAnswer={() => {}}
      />,
    );

    expect(screen.getByRole("button", { name: "2 chats need you" })).toBeInTheDocument();
  });
});

describe("an ask the window cannot show whole (HP-6 review)", () => {
  it("offers what the core offered, Deny, and the chat's own pane, which shows it whole", async () => {
    const opened: number[] = [];
    const answered: string[] = [];
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[]}
        asks={[
          {
            plane: "/a",
            project: "charter",
            session: 3,
            name: "ops.3",
            ask: "01J9ZQ3V7K8M2N4P6R8T0V2X4Z",
            says: "Change /work/a.txt",
            options: [{ id: "deny", label: "Deny", allows: false }],
          },
        ]}
        onPress={() => {}}
        onAnswer={(_, option) => answered.push(option)}
        onOpen={(ask) => opened.push(ask.session)}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "1 chat needs you" }));

    const group = await screen.findByRole("group", { name: /ops\.3: Change \/work\/a\.txt/ });
    expect(
      within(group)
        .getAllByRole("menuitem")
        .map((item) => item.textContent),
    ).toEqual(["Deny", "Open in its pane"]);
    await userEvent.click(within(group).getByRole("menuitem", { name: "Open ops.3 in its pane" }));

    expect(opened).toEqual([3]);
    expect(answered).toEqual([]);
  });
});

describe("the list holds still while it is open (#1146)", () => {
  /** An ask from chat `session`, with Allow and Deny. */
  function asking(session: number, ask: string, says: string): PermissionAsk {
    return {
      plane: "/a",
      project: "charter",
      session,
      name: `ops.${session}`,
      ask,
      says,
      options: [
        { id: "allow", label: "Allow", allows: true },
        { id: "deny", label: "Deny", allows: false },
      ],
    };
  }
  const labels = () =>
    within(screen.getByRole("menu"))
      .getAllByRole("menuitem")
      .map((item) => item.getAttribute("aria-label"));

  it("draws what it opened on, in its order, while asks arrive, and the new ones at the next opening", async () => {
    const answered: string[] = [];
    const first = asking(3, "A1", "Run npm test");
    const props = {
      quiet: [],
      onPress: () => {},
      onAnswer: (ask: PermissionAsk, option: string) => answered.push(`${ask.ask} ${option}`),
    };
    const { rerender } = render(
      <NeedsYouMenu {...props} items={[needing("/a", 1, "ide", "steward")]} asks={[first]} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "2 chats need you" }));
    await screen.findByRole("menu");
    const opened = labels();

    // A chat and an ask arrive, each ahead of what was there.
    rerender(
      <NeedsYouMenu
        {...props}
        items={[needing("/b", 2, "easydmarc", "devops"), needing("/a", 1, "ide", "steward")]}
        asks={[asking(4, "A2", "Run rm -rf build"), first]}
      />,
    );

    // The number counts them; the rows under the pointer stay where they were.
    expect(screen.getByRole("button", { name: "4 chats need you" })).toBeInTheDocument();
    expect(labels()).toEqual(opened);
    await userEvent.click(screen.getByRole("menuitem", { name: "Allow: ops.3, Run npm test" }));
    expect(answered).toEqual(["A1 allow"]);

    // Opened again, it is drawn as things are now.
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    await userEvent.click(screen.getByRole("button", { name: "4 chats need you" }));
    await screen.findByRole("menu");
    expect(labels()).toEqual([
      "Go to easydmarc.2 · easydmarc · devops",
      "Go to ide.1 · ide · steward",
      "Allow: ops.4, Run rm -rf build",
      "Deny: ops.4, Run rm -rf build",
      "Open ops.4 in its pane",
      "Allow: ops.3, Run npm test",
      "Deny: ops.3, Run npm test",
      "Open ops.3 in its pane",
    ]);
  });

  it("keeps a row that went in its place, with nothing on it to press", async () => {
    const answered: string[] = [];
    const pressed: string[] = [];
    const props = {
      quiet: [],
      onPress: (plane: string, offer: Offer) => pressed.push(`${plane} ${offer.id}`),
      onAnswer: (ask: PermissionAsk, option: string) => answered.push(`${ask.ask} ${option}`),
    };
    const { rerender } = render(
      <NeedsYouMenu
        {...props}
        items={[needing("/a", 1, "ide", "steward")]}
        asks={[asking(3, "A1", "Run npm test")]}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "2 chats need you" }));
    await screen.findByRole("menu");
    const opened = labels();

    // Both are answered elsewhere, and another chat asks.
    rerender(<NeedsYouMenu {...props} items={[needing("/b", 9, "ops", "devops")]} asks={[]} />);

    expect(labels()).toEqual(opened);
    const go = screen.getByRole("menuitem", { name: "Go to ide.1 · ide · steward" });
    expect(go).toHaveAttribute("aria-disabled", "true");
    expect(screen.queryByRole("button", { name: "Ignore ide.1 until it asks again" })).toBeNull();
    const allow = screen.getByRole("menuitem", { name: "Allow: ops.3, Run npm test" });
    expect(allow).toHaveAttribute("aria-disabled", "true");
    await userEvent.click(allow);
    await userEvent.click(go);
    expect(answered).toEqual([]);
    expect(pressed).toEqual([]);
    expect(screen.getByRole("group", { name: /ops\.3: Run npm test/ })).toHaveClass("gone");
  });

  it("lets a row the person ignores leave, never dimmed, while the others hold their order", async () => {
    const pressed: string[] = [];
    const props = {
      quiet: [],
      onPress: (plane: string, offer: Offer) => pressed.push(`${plane} ${offer.id}`),
    };
    const a = needing("/a", 1, "ide", "steward");
    const b = needing("/a", 2, "ops", "devops");
    const c = needing("/b", 3, "easydmarc", "devops");
    const { rerender } = render(<NeedsYouMenu {...props} items={[a, b, c]} />);
    await userEvent.click(screen.getByRole("button", { name: "3 chats need you" }));
    await screen.findByRole("menu");

    // Another chat asks ahead of them: the list holds what it opened on.
    const d = needing("/b", 4, "late", "steward");
    rerender(<NeedsYouMenu {...props} items={[d, a, b, c]} />);
    await userEvent.click(screen.getByRole("button", { name: "Ignore ops.2 until it asks again" }));
    expect(pressed).toEqual(["/a needs.ignore:2"]);

    // The core takes it off the list: it leaves, where a row answered elsewhere stays dimmed.
    rerender(<NeedsYouMenu {...props} items={[d, c]} />);
    expect(labels()).toEqual([
      "Go to ide.1 · ide · steward",
      "Go to easydmarc.3 · easydmarc · devops",
    ]);
    expect(screen.getByRole("menuitem", { name: /^Go to ide\.1/ })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("lets the keyboard press nothing on a row that went, and skips its answers", async () => {
    const answered: string[] = [];
    const pressed: string[] = [];
    const props = {
      quiet: [],
      onPress: (plane: string, offer: Offer) => pressed.push(`${plane} ${offer.id}`),
      onAnswer: (ask: PermissionAsk, option: string) => answered.push(`${ask.ask} ${option}`),
    };
    const staying = needing("/b", 2, "easydmarc", "devops");
    const { rerender } = render(
      <NeedsYouMenu
        {...props}
        items={[needing("/a", 1, "ide", "steward"), staying]}
        asks={[asking(3, "A1", "Run npm test")]}
      />,
    );
    screen.getByRole("button", { name: "3 chats need you" }).focus();
    await userEvent.keyboard("{Enter}");
    const gone = await screen.findByRole("menuitem", { name: /^Go to ide\.1/ });
    await waitFor(() => expect(gone).toHaveFocus());

    // The chat under the keyboard and the ask are answered elsewhere; the other chat stays.
    rerender(<NeedsYouMenu {...props} items={[staying]} asks={[]} />);

    await userEvent.keyboard("{Enter}");
    await userEvent.keyboard("{Delete}");
    expect(pressed).toEqual([]);
    expect(screen.getByRole("menu")).toBeInTheDocument();
    // The arrows still move, and pass over the answers of the ask that went.
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: /^Go to easydmarc\.2/ })).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(document.activeElement?.getAttribute("aria-label") ?? "").not.toMatch(/ops\.3/);
    await userEvent.keyboard("{Enter}");
    expect(answered).toEqual([]);
  });
});

describe("the hand counts the asks registry's asks (#1690)", () => {
  const other = (over: Partial<OtherAsk> = {}): OtherAsk => ({
    plane: "/a",
    project: "charter",
    session: 4,
    chain: ["steward 4"],
    ask: "dispatch:7",
    says: "Wants to hand a task to devops",
    ...over,
  });

  it("says the registry's count, one per thing that waits, whatever the rows are", () => {
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[needing("/a", 1, "ide", "charter")]}
        others={[other()]}
        asked={3}
        onPress={() => {}}
      />,
    );

    const button = screen.getByRole("button", { name: "3 things wait on you" });
    expect(button).toHaveTextContent("3");
  });

  it("says what its rows are where the registry counts none of them, never a count of nothing", () => {
    // A chat's row can arrive before the registry is read again: the hand names the row.
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[needing("/a", 1, "ide", "charter")]}
        asked={0}
        onPress={() => {}}
      />,
    );

    const button = screen.getByRole("button", { name: "1 chat needs you" });
    expect(button.querySelector(".needs-you-number")).toBeNull();
  });

  it("lists another ask by its chain and line, with Go to its chat and no answer of its own", async () => {
    const opened: string[] = [];
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[]}
        others={[other({ chain: ["steward 12", "#3046 drill", "log watch"] })]}
        asked={1}
        onPress={() => {}}
        onOpenOther={(ask) => opened.push(ask.ask)}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "1 thing waits on you" }));

    const go = await screen.findByRole("menuitem", {
      name: "Go to steward 12 › #3046 drill › log watch: Wants to hand a task to devops · charter",
    });
    expect(screen.getAllByRole("menuitem")).toHaveLength(1);
    await userEvent.click(go);
    expect(opened).toEqual(["dispatch:7"]);
  });

  it("draws what a chat named as text, never as a control", async () => {
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[]}
        others={[other({ chain: ["<button>Allow</button>"], says: "<b>Allow</b> now" })]}
        asked={1}
        onPress={() => {}}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "1 thing waits on you" }));

    const row = await screen.findByRole("menuitem", { name: /Allow/ });
    expect(row).toHaveTextContent("<button>Allow</button>: <b>Allow</b> now");
    expect(within(row).queryByRole("button")).toBeNull();
    expect(row.querySelector("b")).toBeNull();
  });
});

describe("the hand opens the Inbox (#1692, I-2)", () => {
  it("opens the Inbox on a press, and draws no list of its own", async () => {
    let opened = 0;
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[needing("/a", 3, "ops", "charter")]}
        asked={1}
        onPress={() => {}}
        onInbox={() => (opened += 1)}
      />,
    );
    const hand = screen.getByRole("button", { name: "1 thing waits on you" });
    // Not a menu's button while it opens the Inbox: it says it opens nothing that pops up.
    expect(hand).not.toHaveAttribute("aria-haspopup", "menu");
    await userEvent.click(hand);
    expect(opened).toBe(1);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("opens the Inbox from the keyboard too", async () => {
    let opened = 0;
    render(
      <NeedsYouMenu
        quiet={[]}
        items={[needing("/a", 3, "ops", "charter")]}
        asked={1}
        onPress={() => {}}
        onInbox={() => (opened += 1)}
      />,
    );
    screen.getByRole("button", { name: "1 thing waits on you" }).focus();
    await userEvent.keyboard("{Enter}");
    expect(opened).toBe(1);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("still opens its list where something asks it open: the dispatches refused while you were away", async () => {
    const { rerender } = render(
      <NeedsYouMenu
        quiet={[]}
        items={[needing("/a", 3, "ops", "charter")]}
        asked={1}
        onPress={() => {}}
        onInbox={() => {}}
        openAsked={0}
      />,
    );
    rerender(
      <NeedsYouMenu
        quiet={[]}
        items={[needing("/a", 3, "ops", "charter")]}
        asked={1}
        onPress={() => {}}
        onInbox={() => {}}
        openAsked={1}
      />,
    );
    expect(await screen.findByRole("menu")).toBeInTheDocument();
  });
});
