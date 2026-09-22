import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Health, onTheLine, useDoctor, type DoctorState } from "./Doctor";
import type { DoctorReport, DoctorRow } from "./bindings";

/**
 * **The doctor on the status line**: what the button says, and what opening it asks the core.
 *
 * The rows themselves are the core's and are compared with `charter doctor --json` in
 * `app/src-tauri/src/doctor.rs`; what is here is the window's half — which rows are counted,
 * which are not, and which depth of doctor each gesture asks for.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

function row(name: string, status: DoctorRow["status"], over: Partial<DoctorRow> = {}): DoctorRow {
  return { name, status, detail: `${name} said`, hint: `${name} fix`, checked: true, ...over };
}

/** A row this build does not run — a WARN with `checked: false`, as `Row::deferred` marks it. */
const deferred = (name: string) =>
  row(name, "warn", { detail: "not checked (not ported)", checked: false });

function report(rows: DoctorRow[], over: Partial<DoctorReport> = {}): DoctorReport {
  return {
    rows,
    app_rows: [],
    full: false,
    path: "/usr/bin:/bin:/usr/sbin:/sbin",
    ...over,
  };
}

/** The app's own row — the one `charter doctor` does not print. */
const footerRow = (status: DoctorRow["status"] = "warn"): DoctorRow =>
  row("chat footer", status, {
    detail: "their settings.json fills Claude Code's status line, so the gauge stays dark",
  });

function state(over: Partial<DoctorState> = {}): DoctorState {
  return { running: false, run: () => {}, ...over };
}

const button = () => screen.getByTestId("status-doctor");

describe("the doctor's button", () => {
  it("counts the blockers", () => {
    render(<Health doctor={state({ report: report([row("git", "fail"), row("a", "fail")]) })} />);

    expect(button().textContent).toContain("2 blockers");
    expect(button()).toHaveAccessibleName(/2 blockers/);
  });

  it("does not count a row this build never runs as a warning", () => {
    // About twenty of these on every plane. Counted, the button would say "20 warnings" for
    // ever and the one real warning would never stand out. Measured by deleting the
    // `row.checked` filter: this goes red with "21 warnings".
    const rows = [row("git", "warn"), ...Array.from({ length: 20 }, (_, i) => deferred(`d${i}`))];

    render(<Health doctor={state({ report: report(rows) })} />);

    expect(button().textContent).toContain("1 warning");
    expect(button().textContent).not.toContain("21");
    expect(button()).toHaveAccessibleName(/20 not checked by this build/);
  });

  it("draws no number and no tick when nothing is wrong among what was checked", () => {
    // The footer's rule — zero draws nothing — and ADR 0013's: twenty checks did not run, so
    // "all clear" is a claim this line cannot make.
    render(<Health doctor={state({ report: report([row("git", "ok"), deferred("vaults")]) })} />);

    expect(button().textContent?.trim()).toBe("Doctor");
    expect(button().textContent).not.toContain("✓");
    expect(button()).toHaveAccessibleName(/nothing wrong among the checks this build runs/);
  });

  it("says a blocker before any warning", () => {
    const said = onTheLine(state({ report: report([row("a", "warn"), row("b", "fail")]) }));

    expect(said.said).toBe("1 blocker");
    expect(said.tone).toBe("fail");
  });

  it("spins only while it is checking", () => {
    const { rerender } = render(<Health doctor={state({ running: true })} />);
    expect(button().querySelector(".spinning")).not.toBeNull();

    rerender(<Health doctor={state({ report: report([row("git", "ok")]) })} />);
    expect(button().querySelector(".spinning")).toBeNull();
  });

  it("says it could not run rather than drawing a verdict", () => {
    render(<Health doctor={state({ trouble: "no plane /x is open" })} />);

    expect(button()).toHaveAccessibleName(/could not run: no plane \/x is open/);
    expect(button().textContent).not.toMatch(/blocker|warning/);
  });
});

describe("the app's own rows", () => {
  it("counts a warning about this app's own chats, beside the table's", () => {
    render(
      <Health
        doctor={state({ report: report([row("git", "ok")], { app_rows: [footerRow()] }) })}
      />,
    );

    expect(button().textContent).toContain("1 warning");
  });

  it("draws them under their own heading, because charter doctor does not print them", async () => {
    render(
      <Health
        doctor={state({ report: report([row("git", "ok")], { app_rows: [footerRow()] }) })}
      />,
    );

    await userEvent.click(button());

    const dialog = await screen.findByRole("dialog");
    const ours = within(dialog).getByRole("region", { name: "This app" });
    expect(within(ours).getByText("chat footer")).toBeInTheDocument();
    expect(within(ours).getByText(/gauge stays dark/)).toBeInTheDocument();
    // And it is not counted twice, in the table's own sections.
    expect(within(dialog).getAllByText("chat footer")).toHaveLength(1);
  });

  it("draws no heading for a core that sends none", async () => {
    render(<Health doctor={state({ report: report([row("git", "ok")]) })} />);

    await userEvent.click(button());

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).queryByRole("region", { name: "This app" })).toBeNull();
  });
});

describe("the doctor's dialog", () => {
  it("asks for the full doctor when the operator opens it, and lists every row by verdict", async () => {
    const run = vi.fn();
    const rows = [
      row("schema", "fail"),
      row("git", "warn"),
      row("identity", "ok"),
      deferred("vaults"),
    ];
    render(<Health doctor={state({ run, report: report(rows) })} />);

    await userEvent.click(button());

    // Opening it is the asking: the one gesture that may probe a harness.
    expect(run).toHaveBeenCalledWith(true);
    const dialog = await screen.findByRole("dialog");
    expect(
      within(within(dialog).getByRole("region", { name: "Blockers" })).getByText("schema"),
    ).toBeInTheDocument();
    expect(
      within(within(dialog).getByRole("region", { name: "Warnings" })).getByText("git"),
    ).toBeInTheDocument();
    expect(
      within(within(dialog).getByRole("region", { name: "Passed" })).getByText("identity"),
    ).toBeInTheDocument();
    // Drawn, never dropped — but under their own heading and not among the warnings.
    expect(within(dialog).getByText(/Not checked by this build \(1\)/)).toBeInTheDocument();
    expect(
      within(within(dialog).getByRole("region", { name: "Warnings" })).queryByText("vaults"),
    ).toBeNull();
    // A green row's hint is not drawn, as the CLI's table does not draw it.
    expect(within(dialog).queryByText("identity fix")).toBeNull();
    expect(within(dialog).getByText("schema fix")).toBeInTheDocument();
  });

  it("says the unported rows' shared hint once, not once a row", async () => {
    // Seen on the real app: twenty-four rows each repeating the same two sentences.
    const same = { hint: "This charter does not run this check yet." };
    const rows = [
      row("vaults", "warn", { ...same, checked: false }),
      row("mcp", "warn", { ...same, checked: false }),
      row("plugin", "warn", { ...same, checked: false }),
    ];
    render(<Health doctor={state({ report: report(rows) })} />);

    await userEvent.click(button());

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getAllByText("This charter does not run this check yet.")).toHaveLength(
      1,
    );
  });

  it("shows the PATH the app was answered with, which is what a Finder launch gets wrong", async () => {
    render(<Health doctor={state({ report: report([row("git", "ok")]) })} />);

    await userEvent.click(button());

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("/usr/bin:/bin:/usr/sbin:/sbin")).toBeInTheDocument();
  });

  it("says which depth it is showing", async () => {
    const { rerender } = render(<Health doctor={state({ report: report([row("git", "ok")]) })} />);
    await userEvent.click(button());
    expect(await screen.findByText(/harness profiles are not probed/)).toBeInTheDocument();

    rerender(<Health doctor={state({ report: report([row("git", "ok")], { full: true }) })} />);
    expect(screen.getByText(/each harness profile probed/)).toBeInTheDocument();
  });
});

describe("useDoctor", () => {
  it("runs the preflight once when the project opens", async () => {
    const asked: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "plane_doctor") {
        asked.push(args);
        return report([row("git", "ok")]);
      }
      return null;
    });

    const { result, rerender } = renderHook(() => useDoctor(PLANE));
    await waitFor(() => expect(result.current.report).toBeDefined());
    rerender();
    rerender();

    expect(asked).toEqual([{ plane: PLANE, full: false }]);
  });

  it("makes no report out of a core that answered nothing", async () => {
    // Every whole-window test in this repo mocks the core with `return null` for a command it
    // does not care about. A verdict drawn out of that null would be a doctor made of nothing.
    mockIPC(() => null);

    const { result } = renderHook(() => useDoctor(PLANE));
    await waitFor(() => expect(result.current.running).toBe(false));

    expect(result.current.report).toBeUndefined();
  });

  it("makes no report out of an answer that is not one", async () => {
    // `App.test.tsx` answers every command it does not care about with `[]`. That reached the
    // verdict as a report with no rows and took the window down on CI (and not locally —
    // it lost a race there).
    mockIPC(() => []);

    const { result } = renderHook(() => useDoctor(PLANE));
    await waitFor(() => expect(result.current.running).toBe(false));

    expect(result.current.report).toBeUndefined();
  });

  it("keeps the newest answer when an older ask comes back after it", async () => {
    // The preflight asked at open and the full doctor asked by opening the dialog can come
    // back in either order. The full one must not be overwritten by a preflight that started
    // first and finished second.
    const answers: Record<string, (value: DoctorReport) => void> = {};
    mockIPC((cmd, args) => {
      if (cmd !== "plane_doctor") return null;
      const full = (args as { full: boolean }).full;
      return new Promise<DoctorReport>((resolve) => {
        answers[String(full)] = resolve;
      });
    });

    const { result } = renderHook(() => useDoctor(PLANE));
    await waitFor(() => expect(answers.false).toBeDefined());
    result.current.run(true);
    await waitFor(() => expect(answers.true).toBeDefined());

    answers.true(report([row("git", "ok")], { full: true }));
    await waitFor(() => expect(result.current.report?.full).toBe(true));
    answers.false(report([row("git", "fail")], { full: false }));
    // Give the late answer every chance to land.
    await new Promise((resolve) => setTimeout(resolve, 20));

    expect(result.current.report?.full).toBe(true);
    expect(result.current.running).toBe(false);
  });
});
