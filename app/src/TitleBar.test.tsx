import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { LEAST, leastAt } from "./fits";
import { DEFAULT_TEXT } from "./textSize";
import { TitleBar } from "./TitleBar";
import type { About, Moved, OpenChat, PlaneSaving, RepoSaving } from "./bindings";

/**
 * **The title bar's own rules**, on the component, and **what the bar holds in the window**
 * (ADR 0054), on the real `App` with the core mocked.
 *
 * On the component: that the controls are on the bar and are buttons, what it reserves for the
 * window controls, and that About draws what the core answered out of the changelog rather
 * than a list of its own. In the window: that the project strip is in the bar and nowhere else,
 * with its counts, its show-more and its `+`; that the breadcrumb is gone and the running count
 * is on the status line; and that the right-hand end is still there.
 * `app/e2e/specs/title-bar.e2e.ts` asks what jsdom cannot — whether the bar is really the
 * first thing in the window, whether a stretch of it is left to grab, and whether About names
 * the version the real build announces.
 *
 * **What nothing here proves is that the window moves when the bar is dragged.** That is not a
 * gap in this file: WebDriver dispatches a synthetic event and performs no default action
 * (`docs/ui-primitives.md`), and even a synthetic `mousedown` that reached Tauri's handler
 * would end in an IPC call rather than an observable drag. What is measured instead — here and
 * again in the shipped app — is the two facts that handler reads out of the DOM
 * (`tauri/src/window/scripts/drag.js`): the bar carries `data-tauri-drag-region="deep"`, and
 * every control inside it is something the walk stops at. `BUTTON` is in its `CLICKABLE_TAGS`,
 * so the tag is what does that and no attribute of ours is load-bearing for it — About's
 * `tabIndex={0}` is WebKit's rule (charter-app#186) and the update item's trigger has none,
 * which is charter-app#189's. The third fact, the `core:window:allow-start-dragging`
 * permission, is in `capabilities/default.json` and is a runtime refusal rather than anything
 * a test here can read.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("the bar itself", () => {
  it("is a drag region its own controls are not", () => {
    // Tauri's handler walks the composed path up from what was pressed and returns false at
    // the first clickable element — where `BUTTON` is in its `CLICKABLE_TAGS`, so the tag is
    // what stops it. `deep` rather than the bare attribute, because a bare one drags only on a
    // DIRECT press of the element carrying it: on a bar made of text spans that is everywhere
    // except on its own words.
    render(<TitleBar />);

    expect(screen.getByTestId("title-bar")).toHaveAttribute("data-tauri-drag-region", "deep");
    const about = screen.getByTestId("title-about");
    expect(about.tagName).toBe("BUTTON");
    // And it is not itself a drag region, which would make the button drag the window.
    expect(about).not.toHaveAttribute("data-tauri-drag-region");
  });

  it("gives the button it adds the tabindex WebKit's tab sequence needs", () => {
    // Separate from the drag rule above, and deliberately: the two are different rules that
    // happen to read the same attribute. This one is charter-app#186's — WebKit leaves a
    // `<button>` out of the tab sequence unless its `tabindex` is written down — and it
    // applies to every button this change added. The update item's trigger beside it has
    // none, which is charter-app#189's open question about the whole window outside its
    // dialogs, and is not something this bar should answer on one button.
    render(<TitleBar />);

    expect(screen.getByTestId("title-about")).toHaveAttribute("tabindex", "0");
  });

  it("reserves nothing until the core says how much the system has already spent", () => {
    // The answer lands a frame after the first paint (`useTitleBarRoom` says why it is not
    // awaited before the render), and a bar that guessed 78 px on Linux would have a gap in it
    // that nothing filled.
    render(<TitleBar />);
    expect(screen.getByTestId("title-bar").style.getPropertyValue("--window-controls")).toBe("0px");
    cleanup();

    render(<TitleBar room={{ overlaid: true, reserved: 78 }} />);
    expect(screen.getByTestId("title-bar").style.getPropertyValue("--window-controls")).toBe(
      "78px",
    );
    expect(screen.getByTestId("title-bar")).toHaveAttribute("data-overlaid", "yes");
  });

  it("draws no update item for a window that wired no updater", () => {
    // The same rule the status line's own controls follow: a caller with nothing to offer
    // offers nothing, rather than a control that answers a press with nothing.
    render(<TitleBar />);

    expect(screen.queryByTestId("status-update")).not.toBeInTheDocument();
    expect(screen.getByTestId("title-about")).toBeInTheDocument();
  });
});

describe("About Charter", () => {
  const ABOUT: About = {
    version: "0.1.0",
    build: { kind: "release" },
    notes: {
      version: "0.1.0",
      date: "2026-09-23",
      markdown: "### Added\n\n- **Tabs** hold views.\n- The long story.",
    },
  };

  /** What the core was asked, so a test can prove the dialog reads it rather than a list of
   *  its own. */
  function core(answer: About | Error = ABOUT) {
    const asked: string[] = [];
    mockIPC((cmd) => {
      asked.push(cmd);
      if (cmd === "about_charter") {
        if (answer instanceof Error) throw answer;
        return answer;
      }
      return null;
    });
    return asked;
  }

  it("asks nothing until it is opened, because a launch does not pay for a dialog nobody opened", async () => {
    const asked = core();

    render(<TitleBar />);
    await screen.findByTestId("title-about");

    expect(asked).not.toContain("about_charter");
  });

  it("names the version this build is and draws what it brought as Markdown", async () => {
    core();
    render(<TitleBar />);

    await userEvent.click(screen.getByTestId("title-about"));
    const dialog = await screen.findByRole("dialog");

    expect(within(dialog).getByRole("heading", { name: "About Charter" })).toBeVisible();
    expect(await within(dialog).findByTestId("about-version")).toHaveTextContent("0.1.0");
    expect(dialog).toHaveTextContent("This is Charter 0.1.0, released 2026-09-23.");
    expect(within(dialog).getByRole("heading", { name: "What 0.1.0 brought" })).toBeVisible();
    expect(within(dialog).getByRole("heading", { name: "Added" })).toBeVisible();
    expect(within(dialog).getByText("Tabs").tagName).toBe("STRONG");
    expect(within(dialog).getAllByRole("listitem")).toHaveLength(2);
  });

  it("says a dev build is one, of the next version, and shows what is unreleased", async () => {
    core({
      version: "0.2.0-dev.42",
      build: { kind: "dev", of: "0.2.0" },
      notes: { version: "Unreleased", date: null, markdown: "- Coming next." },
    });
    render(<TitleBar />);

    await userEvent.click(screen.getByTestId("title-about"));
    const dialog = await screen.findByRole("dialog");

    expect(await within(dialog).findByTestId("about-version")).toHaveTextContent("0.2.0-dev.42");
    expect(dialog).toHaveTextContent("a dev build of 0.2.0");
    expect(within(dialog).getByRole("heading", { name: "Not released yet" })).toBeVisible();
    expect(within(dialog).getByRole("listitem")).toHaveTextContent("Coming next.");
  });

  it("says plainly when the changelog has no section for this version", async () => {
    core({ version: "0.3.0", build: { kind: "unlisted" }, notes: null });
    render(<TitleBar />);

    await userEvent.click(screen.getByTestId("title-about"));
    const dialog = await screen.findByRole("dialog");

    expect(await within(dialog).findByTestId("about-version")).toHaveTextContent("0.3.0");
    expect(dialog).toHaveTextContent("The changelog this build carries has no section for it.");
    expect(within(dialog).queryByRole("listitem")).toBeNull();
  });

  it("says an unreleased section with nothing in it has nothing, rather than drawing a blank", async () => {
    core({
      version: "0.2.0-dev.1",
      build: { kind: "dev", of: "0.2.0" },
      notes: { version: "Unreleased", date: null, markdown: "" },
    });
    render(<TitleBar />);

    await userEvent.click(screen.getByTestId("title-about"));
    const dialog = await screen.findByRole("dialog");

    expect(await within(dialog).findByText("Nothing is recorded for it yet.")).toBeVisible();
  });

  it("points at the releases page, and never at Python charter's news", async () => {
    core();
    render(<TitleBar />);

    await userEvent.click(screen.getByTestId("title-about"));
    const dialog = await screen.findByRole("dialog");

    await within(dialog).findByTestId("about-version");
    expect(within(dialog).getByRole("link", { name: "releases page" })).toHaveAttribute(
      "href",
      "https://github.com/diazoxide/charter/releases",
    );
    expect(dialog).not.toHaveTextContent("charter news");
  });

  it("says what went wrong rather than drawing an empty list, when the core refuses", async () => {
    // The changelog is compiled in so it cannot be missing, but a command can always fail —
    // and an About dialog that answered a failure with a blank panel would read as a version
    // that brought nothing.
    core(new Error("no changelog"));
    render(<TitleBar />);

    await userEvent.click(screen.getByTestId("title-about"));
    const dialog = await screen.findByRole("dialog");

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      /charter could not read what this version brought/,
    );
  });

  it("asks once, because the changelog ships in the binary and cannot change while it runs", async () => {
    const asked = core();
    render(<TitleBar />);

    await userEvent.click(screen.getByTestId("title-about"));
    await screen.findByTestId("about-version");
    await userEvent.keyboard("{Escape}");
    await userEvent.click(screen.getByTestId("title-about"));
    await screen.findByTestId("about-version");

    expect(asked.filter((cmd) => cmd === "about_charter")).toHaveLength(1);
  });
});

describe("the save indicator (charter-app#294)", () => {
  function saving(over: Partial<PlaneSaving> = {}): PlaneSaving {
    return {
      stage: "changed",
      changed: ["a.md", "b.md", "c.md"],
      ahead: 0,
      pr: null,
      blocked: null,
      branch: "main",
      pushes: true,
      behind: 0,
      pushFailed: null,
      live: [],
      conflicts: [],
      notice: null,
      mode: "push",
      modeFrom: "charter.toml",
      journal: [],
      ...over,
    };
  }

  /** A workspace repo at `stage`, with everything not under test left plain. */
  function repo(name: string, stage: string): RepoSaving {
    return {
      name,
      mode: "pr",
      modeFrom: "charter.toml",
      autosave: false,
      stage,
      branch: "feature",
      changed: 0,
      ahead: 0,
      pr: null,
      blocked: null,
      pushes: true,
      target: "main",
      ownBranch: false,
    };
  }

  it("says where the project's unsaved work sits, and opens the Saving tab when pressed", async () => {
    const opened: string[] = [];
    render(
      <TitleBar
        save={{
          saving: saving(),
          busy: false,
          onOpen: () => opened.push("open"),
          onSave: () => {},
        }}
      />,
    );

    const bar = screen.getByTestId("title-bar");
    const where = within(bar).getByRole("button", { name: "Saving: 3 changed" });
    expect(where.tabIndex).toBe(0);
    await userEvent.click(where);
    expect(opened).toEqual(["open"]);
  });

  it("saves from the bar, and offers no save when there is nothing to take", async () => {
    const saved: string[] = [];
    render(
      <TitleBar
        save={{ saving: saving(), busy: false, onOpen: () => {}, onSave: () => saved.push("save") }}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Save the project" }));
    expect(saved).toEqual(["save"]);
    cleanup();

    render(
      <TitleBar
        save={{
          saving: saving({ stage: "saved", changed: [] }),
          busy: false,
          onOpen: () => {},
          onSave: () => {},
        }}
      />,
    );
    expect(screen.getByRole("button", { name: "Saving: Saved" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Save the project" })).toBeNull();
  });

  it("shows what came in as its own ↓N, outside the words the cap cuts (charter#403)", () => {
    // The operator's ruling: the words keep their 12rem cap, and the incoming count is never
    // cut with them. A blocked stage is a whole sentence, which is exactly what the cap cuts.
    const blocked = "this plane is not a git repository, so there is nothing to commit to";
    render(
      <TitleBar
        save={{
          saving: saving({ stage: "blocked", blocked, changed: [], behind: 12 }),
          busy: false,
          onOpen: () => {},
          onSave: () => {},
        }}
      />,
    );

    // Said in full to whoever reads the button's name or its tooltip.
    const where = screen.getByRole("button", {
      name: `Saving: Blocked: ${blocked} · 12 incoming`,
    });
    const incoming = within(where).getByText("↓12");
    expect(incoming.closest(".save-indicator-words")).toBeNull();
    const words = where.querySelector(".save-indicator-words");
    expect(words?.textContent).toBe(`Blocked: ${blocked}`);
  });

  it("carries the same ↓N when the furthest-back stage is a repo's", () => {
    render(
      <TitleBar
        save={{
          saving: saving({ stage: "saved", changed: [], behind: 3 }),
          repos: [repo("svc", "pr-open"), repo("web", "pr-open")],
          busy: false,
          onOpen: () => {},
          onSave: () => {},
        }}
      />,
    );

    const where = screen.getByRole("button", {
      name: "Saving: 2 repos waiting on their pull requests · 3 incoming",
    });
    expect(within(where).getByText("↓3").closest(".save-indicator-words")).toBeNull();
    expect(where.querySelector(".save-indicator-words")?.textContent).toBe(
      "2 repos waiting on their pull requests",
    );
  });

  it("draws no ↓ while nothing has come in", () => {
    render(
      <TitleBar
        save={{ saving: saving({ behind: 0 }), busy: false, onOpen: () => {}, onSave: () => {} }}
      />,
    );
    expect(screen.queryByText(/^↓/)).toBeNull();
  });

  it("draws nothing without a project to save", () => {
    render(<TitleBar />);
    expect(screen.queryByRole("button", { name: /^Saving:/ })).toBeNull();
  });
});

describe("the title bar in the window, which holds the project strip (ADR 0054)", () => {
  const ONE = "/home/dev/one";
  const TWO = "/home/dev/two";

  /** A chat the core says a project has open, working in that project's `alpha`. */
  function chat(root: string, session: number, name: string): OpenChat {
    return {
      session,
      name,
      cwd: `${root}/workspaces/alpha`,
      harness: null,
      in_front: session === 1,
      resumed: null,
      fresh: null,
      profile: null,
      persona: null,
      unreported: null,
      pinned: false,
      label: null,
      from: null,
    };
  }

  /**
   * A core holding two projects, `one` in front, each with a workspace `alpha` and one chat in
   * it. Answers `chat-moved` to every project listening, the way the core pushes one.
   */
  function core() {
    const listeners = new Map<string, number[]>();
    mockIPC((cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      if (cmd === "plugin:event|listen") {
        const { event, handler } = given as unknown as { event: string; handler: number };
        listeners.set(event, [...(listeners.get(event) ?? []), handler]);
        return 1;
      }
      const plane = (given.plane as string | undefined) ?? "";
      if (cmd === "plane_at_launch") return { plane: null, from: null, why: null };
      if (cmd === "planes_to_restore")
        return { windows: [{ planes: [ONE, TWO], active: 0 }], dropped: [] };
      if (cmd === "open_plane") return { plane: given.path, ask: null };
      if (cmd === "plane_sidebar")
        return {
          root: plane,
          workspaces: [
            {
              name: "alpha",
              path: `${plane}/workspaces/alpha`,
              vision: "",
              todos: [],
              chats: [chat(plane, 1, `${plane.split("/").pop()}.1`)],
            },
          ],
          personas: [],
          persona: null,
          unfiled: [],
        };
      if (cmd === "opened_chats") return [chat(plane, 1, `${plane.split("/").pop()}.1`)];
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "recent_planes") return { planes: [], dropped: [], forgetful: null };
      if (cmd === "extensions_on") return [];
      return null;
    });
    return {
      /** Says chat 1 in `plane` is now in `state`, and whether it is asking for you. */
      move(plane: string, state: Moved["state"], sequence: number) {
        const moved: Moved = {
          plane,
          session: 1,
          state,
          needs_you: state === "waiting",
          queue: state === "waiting" ? [1] : [],
          moved_at: sequence,
          sequence,
          reports: [],
        };
        for (const handler of listeners.get("chat-moved") ?? [])
          window.__TAURI_INTERNALS__.runCallback(handler, {
            event: "chat-moved",
            id: 1,
            payload: moved,
          });
      },
    };
  }

  const bar = () => screen.getByTestId("title-bar");
  const strip = () => screen.getByRole("tablist", { name: "Projects" });

  it("holds the project tabs, the window's only project strip, each with its needs-you count", async () => {
    const { move } = core();
    render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
    await waitFor(() => expect(within(strip()).getByText("two")).toBeInTheDocument());

    move(TWO, "waiting", 1);

    expect(await within(bar()).findByLabelText("1 chats need you in two")).toBeInTheDocument();
    expect(within(bar()).getByRole("tablist", { name: "Projects" })).toBe(strip());
  });

  it("holds the project strip's show-more, with what it hides that needs you, and its +", async () => {
    // Room for one project tab, so `two` is behind the show-more button. jsdom lays nothing
    // out (#149): the strip answers `clientWidth` by its name, and nothing else is measured.
    const was = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
    Object.defineProperty(HTMLElement.prototype, "clientWidth", {
      configurable: true,
      get(this: HTMLElement) {
        return this.getAttribute("aria-label") === "Projects"
          ? leastAt(LEAST.project, DEFAULT_TEXT.window)
          : 0;
      },
    });
    try {
      const { move } = core();
      render(
        <StrictMode>
          <App />
        </StrictMode>,
      );
      await waitFor(() => expect(within(strip()).getByText("one")).toBeInTheDocument());

      move(TWO, "waiting", 1);

      expect(
        await within(bar()).findByRole("button", {
          name: "Show 1 project the strip is not showing, where 1 chat needs you",
        }),
      ).toBeInTheDocument();
      expect(within(bar()).getByRole("button", { name: "Open a project…" })).toBeInTheDocument();
      expect(within(bar()).getByRole("button", { name: "New project…" })).toBeInTheDocument();
    } finally {
      if (was) Object.defineProperty(HTMLElement.prototype, "clientWidth", was);
      else Reflect.deleteProperty(HTMLElement.prototype, "clientWidth");
    }
  });

  it("says no breadcrumb: the project tab and the workspace strip say where the window is", async () => {
    // ADR 0054: a crumb repeating both spends the width the project tabs need.
    core();
    render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
    await waitFor(() =>
      expect(screen.getByRole("tablist", { name: "Workspaces" })).toHaveTextContent("alpha"),
    );

    expect(bar()).not.toHaveTextContent("alpha");
    expect(bar()).not.toHaveTextContent(/running|counting chats/);
  });

  it("leaves how many chats are running to the status line of the project in front", async () => {
    const { move } = core();
    render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
    await waitFor(() => expect(within(strip()).getByText("two")).toBeInTheDocument());

    move(ONE, "running", 1);
    // A chat running in a project behind is that project's line's to say, not this one's.
    move(TWO, "running", 2);

    const line = screen.getByTestId("status-line");
    expect(await within(line).findByText("1 chat running")).toBeInTheDocument();
  });

  it("keeps its right-hand end: the needs-you hand, About and the update item", async () => {
    const { move } = core();
    render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
    await waitFor(() => expect(within(strip()).getByText("two")).toBeInTheDocument());

    move(TWO, "waiting", 1);

    expect(
      await within(bar()).findByRole("button", { name: "1 chat needs you" }),
    ).toBeInTheDocument();
    expect(within(bar()).getByTestId("title-about")).toBeInTheDocument();
    expect(within(bar()).getByTestId("status-update")).toBeInTheDocument();
  });

  it("drags from anywhere but its controls, and every project tab is a button it stops at", async () => {
    // Tauri's `deep` handler stops at the first clickable element up the path, and `BUTTON`
    // is one by its tag. A tab drawn as anything else would drag the window from under it.
    core();
    render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
    await waitFor(() => expect(within(strip()).getByText("two")).toBeInTheDocument());

    expect(bar()).toHaveAttribute("data-tauri-drag-region", "deep");
    for (const name of ["one", "two"]) {
      const tab = within(strip()).getByText(name).closest("button");
      expect(tab, `${name}'s tab is not a button`).not.toBeNull();
      expect(bar().contains(tab)).toBe(true);
      expect(tab).not.toHaveAttribute("data-tauri-drag-region");
    }
  });
});
