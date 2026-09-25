import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { TitleBar, Breadcrumb, runningIn, type Crumbs } from "./TitleBar";
import type { About, PlaneSaving } from "./bindings";

/**
 * **The title bar's own rules**, on the components and nothing else.
 *
 * The questions this file can answer are the ones a props-in/markup-out test really settles:
 * what each degraded reading of the breadcrumb SAYS, that *running* means running, that the
 * controls are on the bar and are buttons, and that About draws what the core answered out of
 * the changelog rather than a list of its own. `app/e2e/specs/title-bar.e2e.ts` asks the two things jsdom cannot —
 * whether the bar is really the first thing in the window, and whether About names the version
 * the real build announces.
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

afterEach(() => {
  cleanup();
  clearMocks();
});

/** A breadcrumb with everything answered, which each test then takes one answer away from. */
function crumbs(over: Partial<Crumbs> = {}): Crumbs {
  return {
    project: "charter-app",
    decided: true,
    read: true,
    workspace: "alpha",
    running: 3,
    ...over,
  };
}

/** What the row reads as, with the separators the operator asked for left in. */
function said(): string {
  return screen.getByTestId("title-crumbs").textContent ?? "";
}

describe("the breadcrumb, which says where the window is", () => {
  it("says project, workspace and how many chats are running", () => {
    render(<Breadcrumb crumbs={crumbs()} />);

    expect(said()).toBe("charter-app/alpha/3 chats running");
  });

  it("says one chat rather than 1 chats, because it is a sentence and not a cell", () => {
    render(<Breadcrumb crumbs={crumbs({ running: 1 })} />);

    expect(said()).toBe("charter-app/alpha/1 chat running");
  });

  it("says no chats running rather than dropping the clause, which is NOT the footer's rule", () => {
    // `StatusLine` drops a count at zero — presence is the signal, and a `todo 0` on the line
    // every day is furniture by Friday. This is the third clause of one sentence, and a
    // sentence that stops at the second slash on a quiet morning reads as charter having
    // failed to count rather than as charter having counted none.
    render(<Breadcrumb crumbs={crumbs({ running: 0 })} />);

    expect(said()).toBe("charter-app/alpha/no chats running");
  });

  it("says it is still counting rather than saying zero, before the project has settled", () => {
    // The gap this closes: a plane putting twenty chats back reports an empty `ending` for the
    // first moments of every launch, and "no chats running" over it would be a wrong number
    // nobody could tell from a right one.
    render(<Breadcrumb crumbs={crumbs({ running: undefined })} />);

    expect(said()).toBe("charter-app/alpha/counting chats…");
  });

  it("tells a plane it has not read from a plane that holds no workspace", () => {
    // Two different claims, and the workspace's name cannot carry both — the same split
    // `StatusLine` makes, in the same words, so the top and the bottom of the window agree.
    render(<Breadcrumb crumbs={crumbs({ read: false, workspace: undefined })} />);
    expect(said()).toBe("charter-app/reading the plane…/3 chats running");
    cleanup();

    render(<Breadcrumb crumbs={crumbs({ read: true, workspace: undefined })} />);
    expect(said()).toBe("charter-app/no workspace/3 chats running");
  });

  it("replaces the whole row with a sentence when there is no project, rather than drawing a skeleton", () => {
    // The failure this component is written against: `  /  / ` — two empty segments and two
    // slashes, which says nothing and looks broken.
    render(<Breadcrumb crumbs={crumbs({ project: undefined })} />);

    expect(said()).toBe("No project open");
    expect(said()).not.toContain("/");
  });

  it("says charter is opening before the window has decided whether it holds a project", () => {
    // The opener's own pair of conditions (`App.tsx`'s `openerUp`): a cold launch restoring
    // eight projects holds none for a moment, and "No project open" over it is the same lie
    // the opener is careful not to tell in a larger font.
    render(<Breadcrumb crumbs={crumbs({ project: undefined, decided: false })} />);

    expect(said()).toBe("charter is opening…");
  });

  it("puts the whole reading on the row's title, so what it had to truncate is a hover away", () => {
    // The names truncate in CSS, which jsdom does not run — what is testable is that nothing
    // truncated is unrecoverable.
    render(
      <Breadcrumb crumbs={crumbs({ project: "a-very-long-project-name", workspace: "svc" })} />,
    );

    expect(screen.getByTestId("title-crumbs")).toHaveAttribute(
      "title",
      "a-very-long-project-name / svc / 3 chats running",
    );
  });

  it("hides the separators from a screen reader, which would otherwise read the punctuation", () => {
    render(<Breadcrumb crumbs={crumbs()} />);

    const marks = screen.getByTestId("title-crumbs").querySelectorAll(".crumb-sep");
    expect(marks).toHaveLength(2);
    for (const mark of marks) expect(mark).toHaveAttribute("aria-hidden", "true");
  });
});

describe("what `N chats running` counts", () => {
  const chat = (state: string) => ({ state });

  it("counts a chat that is running and no other state", () => {
    // The operator runs many chats at once and most of them are sitting still. `open` would
    // say 50 all day; `running` is the number that changes when something is happening.
    const report = {
      settled: true,
      ending: [
        chat("running"),
        chat("waiting"),
        chat("done"),
        chat("failed"),
        chat("unknown"),
        chat("running"),
      ],
    };

    expect(runningIn(report)).toBe(2);
  });

  it("counts nothing at all until the project has said what it had open", () => {
    // An empty `ending` before the core has answered is "not yet", never zero — the same
    // distinction `PlaneReport.settled` exists for, and a quit that got it wrong would end
    // every chat it had not heard about.
    expect(runningIn({ settled: false, ending: [] })).toBeUndefined();
    expect(runningIn({ settled: false, ending: [chat("running")] })).toBeUndefined();
    expect(runningIn(undefined)).toBeUndefined();
  });

  it("counts zero once it has settled on nothing, which is an answer", () => {
    expect(runningIn({ settled: true, ending: [] })).toBe(0);
  });
});

describe("the bar itself", () => {
  it("is a drag region its own controls are not", () => {
    // Tauri's handler walks the composed path up from what was pressed and returns false at
    // the first clickable element — where `BUTTON` is in its `CLICKABLE_TAGS`, so the tag is
    // what stops it. `deep` rather than the bare attribute, because a bare one drags only on a
    // DIRECT press of the element carrying it: on a bar made of text spans that is everywhere
    // except on its own words.
    render(<TitleBar crumbs={crumbs()} />);

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
    render(<TitleBar crumbs={crumbs()} />);

    expect(screen.getByTestId("title-about")).toHaveAttribute("tabindex", "0");
  });

  it("reserves nothing until the core says how much the system has already spent", () => {
    // The answer lands a frame after the first paint (`useTitleBarRoom` says why it is not
    // awaited before the render), and a bar that guessed 78 px on Linux would have a gap in it
    // that nothing filled.
    render(<TitleBar crumbs={crumbs()} />);
    expect(screen.getByTestId("title-bar").style.getPropertyValue("--window-controls")).toBe("0px");
    cleanup();

    render(<TitleBar crumbs={crumbs()} room={{ overlaid: true, reserved: 78 }} />);
    expect(screen.getByTestId("title-bar").style.getPropertyValue("--window-controls")).toBe(
      "78px",
    );
    expect(screen.getByTestId("title-bar")).toHaveAttribute("data-overlaid", "yes");
  });

  it("draws no update item for a window that wired no updater", () => {
    // The same rule the status line's own controls follow: a caller with nothing to offer
    // offers nothing, rather than a control that answers a press with nothing.
    render(<TitleBar crumbs={crumbs()} />);

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

    render(<TitleBar crumbs={crumbs()} />);
    await screen.findByTestId("title-about");

    expect(asked).not.toContain("about_charter");
  });

  it("names the version this build is and draws what it brought as Markdown", async () => {
    core();
    render(<TitleBar crumbs={crumbs()} />);

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
    render(<TitleBar crumbs={crumbs()} />);

    await userEvent.click(screen.getByTestId("title-about"));
    const dialog = await screen.findByRole("dialog");

    expect(await within(dialog).findByTestId("about-version")).toHaveTextContent("0.2.0-dev.42");
    expect(dialog).toHaveTextContent("a dev build of 0.2.0");
    expect(within(dialog).getByRole("heading", { name: "Not released yet" })).toBeVisible();
    expect(within(dialog).getByRole("listitem")).toHaveTextContent("Coming next.");
  });

  it("says plainly when the changelog has no section for this version", async () => {
    core({ version: "0.3.0", build: { kind: "unlisted" }, notes: null });
    render(<TitleBar crumbs={crumbs()} />);

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
    render(<TitleBar crumbs={crumbs()} />);

    await userEvent.click(screen.getByTestId("title-about"));
    const dialog = await screen.findByRole("dialog");

    expect(await within(dialog).findByText("Nothing is recorded for it yet.")).toBeVisible();
  });

  it("points at the releases page, and never at Python charter's news", async () => {
    core();
    render(<TitleBar crumbs={crumbs()} />);

    await userEvent.click(screen.getByTestId("title-about"));
    const dialog = await screen.findByRole("dialog");

    await within(dialog).findByTestId("about-version");
    expect(within(dialog).getByRole("link", { name: "releases page" })).toHaveAttribute(
      "href",
      "https://github.com/diazoxide/charter-app/releases",
    );
    expect(dialog).not.toHaveTextContent("charter news");
  });

  it("says what went wrong rather than drawing an empty list, when the core refuses", async () => {
    // The changelog is compiled in so it cannot be missing, but a command can always fail —
    // and an About dialog that answered a failure with a blank panel would read as a version
    // that brought nothing.
    core(new Error("no changelog"));
    render(<TitleBar crumbs={crumbs()} />);

    await userEvent.click(screen.getByTestId("title-about"));
    const dialog = await screen.findByRole("dialog");

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      /charter could not read what this version brought/,
    );
  });

  it("asks once, because the changelog ships in the binary and cannot change while it runs", async () => {
    const asked = core();
    render(<TitleBar crumbs={crumbs()} />);

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
      mode: "push",
      modeFrom: "charter.toml",
      journal: [],
      ...over,
    };
  }

  it("says where the project's unsaved work sits, and opens the Saving tab when pressed", async () => {
    const opened: string[] = [];
    render(
      <TitleBar
        crumbs={crumbs()}
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
        crumbs={crumbs()}
        save={{ saving: saving(), busy: false, onOpen: () => {}, onSave: () => saved.push("save") }}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Save the project" }));
    expect(saved).toEqual(["save"]);
    cleanup();

    render(
      <TitleBar
        crumbs={crumbs()}
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

  it("draws nothing without a project to save", () => {
    render(<TitleBar crumbs={crumbs()} />);
    expect(screen.queryByRole("button", { name: /^Saving:/ })).toBeNull();
  });
});
