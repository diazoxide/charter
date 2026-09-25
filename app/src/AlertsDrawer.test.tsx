import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import type { AlertRow, PlaneAlerts } from "./bindings";
import type { AlertsReading } from "./alerts";
import { AlertsDrawer } from "./AlertsDrawer";

/**
 * **The alerts drawer**, on the component: what it lists for each project, and the states in
 * which charter has less than a full answer. The window's wiring — the status line's button
 * opening it, the core asked for every project — is in `FourRegions.test.tsx`.
 */

afterEach(cleanup);

const A = "/home/dev/alpha-plane";
const B = "/home/dev/beta-plane";
const nameOf = (plane: string) => plane.split("/").pop() ?? plane;

function row(over: Partial<AlertRow> = {}): AlertRow {
  return {
    severity: "warn",
    subject: "front door",
    detail: "ghost — no such persona",
    remedy: "charter persona default <name>",
    ...over,
  };
}

function plane(at: string, alerts: AlertRow[], stopped: string | null = null): PlaneAlerts {
  return { plane: at, alerts, stopped };
}

function draw(reading: AlertsReading, planes: string[] = [A, B]) {
  return render(
    <AlertsDrawer open onOpenChange={() => {}} reading={reading} planes={planes} nameOf={nameOf} />,
  );
}

const project = (name: string) => screen.getByRole("region", { name: `Alerts in ${name}` });

describe("what the window says about this machine", () => {
  it("is listed above the projects, with what fixes it", () => {
    render(
      <AlertsDrawer
        open
        onOpenChange={() => {}}
        reading={{ at: "read", planes: [plane(A, [])] }}
        aboutThisMachine={[
          row({
            subject: "layout",
            detail: 'layout.json: "minimap" is not a region this charter has',
            remedy: "fix layout.json",
          }),
        ]}
        planes={[A]}
        nameOf={nameOf}
      />,
    );

    const machine = screen.getByRole("region", { name: "Alerts about this machine" });
    expect(machine).toHaveTextContent("This machine");
    expect(machine).toHaveTextContent('"minimap" is not a region this charter has');
    expect(within(machine).getByText("fix layout.json").tagName).toBe("CODE");
    const regions = screen.getAllByRole("region");
    expect(regions.indexOf(machine)).toBeLessThan(regions.indexOf(project("alpha-plane")));
  });

  it("is not drawn at all when there is nothing to say", () => {
    draw({ at: "read", planes: [plane(A, [])] }, [A]);

    expect(screen.queryByRole("region", { name: "Alerts about this machine" })).toBeNull();
  });
});

describe("the alerts drawer", () => {
  it("lists every project the window holds, each with its own alerts and what fixes them", () => {
    draw({
      at: "read",
      planes: [
        plane(A, [row()]),
        plane(B, [
          row({
            severity: "bad",
            subject: "plane root",
            detail: "beta-plane · memory commit not pushed",
            remedy: "save the plane, or move the work to a workspace clone",
          }),
        ]),
      ],
    });

    const drawer = screen.getByRole("dialog", { name: "Alerts" });
    expect(within(drawer).getByText(/Every open project, not only the one in front/)).toBeVisible();

    const alpha = project("alpha-plane");
    expect(alpha).toHaveTextContent("front door");
    expect(alpha).toHaveTextContent("ghost — no such persona");
    expect(within(alpha).getByText("charter persona default <name>").tagName).toBe("CODE");
    expect(alpha).not.toHaveTextContent("memory commit");

    const beta = project("beta-plane");
    expect(beta).toHaveTextContent("memory commit not pushed");
    expect(within(beta).getByRole("listitem")).toHaveAttribute("data-severity", "bad");
  });

  it("lists them in the strip's order, with a project only the core holds after them", () => {
    const C = "/home/dev/gamma-plane";
    draw({ at: "read", planes: [plane(C, []), plane(B, []), plane(A, [])] }, [A, B]);

    const names = screen.getAllByRole("region").map((one) => one.getAttribute("aria-label"));
    expect(names).toEqual([
      "Alerts in alpha-plane",
      "Alerts in beta-plane",
      "Alerts in gamma-plane",
    ]);
  });

  it("says nothing needs you in a project read to the end with no alerts", () => {
    draw({ at: "read", planes: [plane(A, []), plane(B, [row()])] });

    expect(project("alpha-plane")).toHaveTextContent("Nothing needs you here.");
    expect(project("beta-plane")).not.toHaveTextContent("Nothing needs you here.");
  });

  it("says where charter stopped looking, keeps what it found, and does not call it nothing", () => {
    draw({
      at: "read",
      planes: [
        plane(A, [], "charter.toml is not valid TOML"),
        plane(B, [row({ subject: "charter" })], "[persona] in charter.toml is a string"),
      ],
    });

    const alpha = project("alpha-plane");
    expect(within(alpha).getByRole("alert")).toHaveTextContent(
      "charter stopped looking here: charter.toml is not valid TOML",
    );
    expect(alpha).not.toHaveTextContent("Nothing needs you here.");

    const beta = project("beta-plane");
    expect(within(beta).getByRole("alert")).toHaveTextContent("there may be more");
    expect(beta).toHaveTextContent("charter");
    expect(within(beta).getAllByRole("listitem")).toHaveLength(1);
  });

  it("says a project opened after the reading has not been read, rather than that it is clear", () => {
    draw({ at: "read", planes: [plane(A, [])] }, [A, B]);

    expect(project("beta-plane")).toHaveTextContent("Not read yet.");
    expect(project("beta-plane")).not.toHaveTextContent("Nothing needs you here.");
  });

  it("says it is still reading before the first answer, and the core's words when it failed", () => {
    const { unmount } = draw({ at: "reading" });
    expect(screen.getByRole("dialog")).toHaveTextContent("Reading every open project…");
    expect(screen.queryByRole("region")).toBeNull();
    unmount();

    draw({ at: "failed", why: "the registry is gone" });
    expect(screen.getByRole("alert")).toHaveTextContent(
      "charter could not read the alerts: the registry is gone",
    );
  });

  it("closes on Escape, on its Close button and on a click outside, and hands the keyboard back", async () => {
    function Window() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>
            Alerts
          </button>
          <AlertsDrawer
            open={open}
            onOpenChange={setOpen}
            reading={{ at: "read", planes: [plane(A, [])] }}
            planes={[A]}
            nameOf={nameOf}
          />
        </>
      );
    }
    render(<Window />);
    const opener = screen.getByRole("button", { name: "Alerts" });

    await userEvent.click(opener);
    await screen.findByRole("dialog", { name: "Alerts" });
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // Radix returns focus to a `Dialog.Trigger`, and the status line's button is not one.
    expect(opener).toHaveFocus();

    await userEvent.click(opener);
    await userEvent.click(await screen.findByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(opener).toHaveFocus();

    await userEvent.click(opener);
    await screen.findByRole("dialog");
    // A drawer asks nothing, so a click on the scrim closes it — unlike the window's questions.
    const scrim = document.querySelector(".asking");
    expect(scrim).not.toBeNull();
    await userEvent.click(scrim as Element);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});
