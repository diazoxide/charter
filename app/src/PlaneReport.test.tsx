import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { PlaneView, type PlaneReport, type WindowDoing } from "./PlaneView";

/**
 * **A project's report reaches whoever is listening now** (#1037). The view remembers the last
 * report it sent so a chat move that changes nothing is not sent again (SC-3); what it
 * remembers has to include who it sent it to and for which project, or a new callback — or the
 * same view told it is another project — would hear nothing until something else moved.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => <div>session {session}</div>,
}));

afterEach(() => {
  cleanup();
  clearMocks();
});

/** A window that does nothing: none of these is pressed here. */
const windowDoes = new Proxy({} as WindowDoing, { get: () => vi.fn() });

/** The same lists every time, so a redraw with a new callback changes nothing else. */
const NO_PROJECTS: never[] = [];
const NO_PINS: never[] = [];

function draw(plane: string, onReport: (plane: string, report: PlaneReport) => void) {
  return (
    <PlaneView
      plane={plane}
      inFront
      projects={NO_PROJECTS}
      pinnedProjects={NO_PINS}
      window={windowDoes}
      onReport={onReport}
    />
  );
}

/** Lets the mocked core's answers land, until a few turns in a row report nothing new. */
async function settle(...heard: ReturnType<typeof vi.fn>[]) {
  const count = () => heard.reduce((sum, one) => sum + one.mock.calls.length, 0);
  for (let quiet = 0, was = -1; quiet < 3;) {
    await act(async () => {});
    quiet = count() === was ? quiet + 1 : 0;
    was = count();
  }
}

describe("a project's report", () => {
  it("is sent again to a new callback, once", async () => {
    mockIPC(() => null);
    const first = vi.fn();
    const view = render(draw("/home/dev/plane", first));
    await settle(first);
    expect(first).toHaveBeenCalled();

    const second = vi.fn();
    view.rerender(draw("/home/dev/plane", second));
    await settle(first, second);
    expect(second).toHaveBeenCalledTimes(1);
    expect(second.mock.calls[0][0]).toBe("/home/dev/plane");

    // The same callback again is the same listener: nothing new to tell it.
    view.rerender(draw("/home/dev/plane", second));
    await settle(first, second);
    expect(second).toHaveBeenCalledTimes(1);
  });

  it("is sent again when the view is told it is another project", async () => {
    mockIPC(() => null);
    const onReport = vi.fn();
    const view = render(draw("/home/dev/one", onReport));
    await settle(onReport);
    const before = onReport.mock.calls.length;

    view.rerender(draw("/home/dev/two", onReport));
    await settle(onReport);
    expect(onReport.mock.calls.slice(before).map(([plane]) => plane)).toContain("/home/dev/two");
  });
});
