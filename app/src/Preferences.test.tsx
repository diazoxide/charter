import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import App from "./App";
import { Preferences } from "./Preferences";
import { forgetThisLaunch } from "./regions";
import { DEFAULT_TEXT, setTextSize, textSizes } from "./textSize";
import { GLOBAL } from "./windowprefs";

/**
 * **Preferences** (charter-app#283): the machine's two text sizes, as a view tab reached from
 * the palette and the app menu, and drawn where the opener is when no project is open.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const PATH = "/home/op/.config/charter/layout.json";
const PLANE = "/home/dev/plane";

beforeEach(() => {
  forgetThisLaunch();
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: PATH, found: false, document: null, trouble: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

const slider = (name: RegExp) => screen.getByRole("slider", { name });

describe("the Preferences tab's body", () => {
  beforeEach(() => mockIPC(() => null));

  it("draws both sizes, and says they are this machine's and where they are kept", () => {
    render(<Preferences />);

    expect(slider(/window text size/i)).toHaveValue(String(DEFAULT_TEXT.window));
    expect(slider(/terminal text size/i)).toHaveValue(String(DEFAULT_TEXT.terminal));
    expect(screen.getByTestId("preferences")).toHaveTextContent("This machine only");
    expect(screen.getByTestId("preferences")).toHaveTextContent(PATH);
  });

  it("applies a size as the slider moves", () => {
    render(<Preferences />);

    fireEvent.change(slider(/window text size/i), { target: { value: "18" } });

    expect(textSizes().window).toBe(18);
    expect(slider(/window text size/i)).toHaveValue("18");
    expect(screen.getByText("18px")).toBeInTheDocument();
  });

  it("redraws when a size changes from somewhere else — a key", () => {
    render(<Preferences />);

    act(() => setTextSize("terminal", 16));

    expect(slider(/terminal text size/i)).toHaveValue("16");
  });

  it("resets one size to its default, and cannot reset one that is already there", async () => {
    render(<Preferences />);
    const reset = () => screen.getByRole("button", { name: `Reset to ${DEFAULT_TEXT.terminal}px` });
    expect(reset()).toBeDisabled();

    act(() => setTextSize("terminal", 20));
    await userEvent.click(reset());

    expect(textSizes().terminal).toBe(DEFAULT_TEXT.terminal);
    expect(reset()).toBeDisabled();
  });
});

function core(plane: string | null) {
  mockIPC(
    (cmd) => {
      if (cmd === "plane_at_launch")
        return { plane, from: plane, why: plane === null ? "no plane here" : null };
      if (cmd === "opened_chats") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "plane_sidebar")
        return { root: plane, personas: [], persona: null, unfiled: [], workspaces: [] };
      return null;
    },
    { shouldMockEvents: true },
  );
}

async function fromThePalette() {
  await userEvent.keyboard("{F2}");
  await screen.findByRole("dialog", { name: "Command palette" });
  await userEvent.keyboard("Preferences");
  await userEvent.keyboard("{Enter}");
}

const preferenceTabs = () =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .queryAllByRole("tab")
    .filter((tab) => tab.textContent?.includes("Preferences"));

describe("the Preferences tab, from the window", () => {
  it("opens from the palette as a view tab of its own on the project in front, once", async () => {
    core(PLANE);
    render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
    await screen.findByRole("tab", { name: /plane/ });

    await fromThePalette();
    expect(await screen.findByTestId("preferences")).toBeInTheDocument();
    expect(preferenceTabs()).toHaveLength(1);

    await fromThePalette();
    expect(preferenceTabs()).toHaveLength(1);
  });

  it("opens from the app menu's Preferences… (⌘,), which the core says with an event", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });
    // The listeners register asynchronously; give them a turn.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    await act(async () => {
      await emit("preferences-asked");
    });

    expect(await screen.findByTestId("preferences")).toBeInTheDocument();
    expect(preferenceTabs()).toHaveLength(1);
  });

  it("is drawn where the opener is when no project is open, and Done goes back to it", async () => {
    core(null);
    render(<App />);
    const opener = () => screen.queryByRole("heading", { name: /project/, level: 1 });
    await waitFor(() => expect(opener()).toBeInTheDocument());

    await fromThePalette();

    expect(await screen.findByTestId("preferences")).toBeInTheDocument();
    expect(opener()).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(screen.queryByTestId("preferences")).not.toBeInTheDocument();
    expect(opener()).toBeInTheDocument();
  });
});
