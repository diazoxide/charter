import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { DoctorReport, DoctorRow } from "./bindings";
import { forgetThisLaunch } from "./regions";

/**
 * **A Notice that carries a fix with a form draws that form** (#1250), against the whole
 * window.
 *
 * The doctor's `git identity` finding stands as a Notice under the strip, carrying its fix id,
 * `git-identity`. That fix takes a name and an email (`FIXES_WITH_A_FORM`), so pressing the
 * Notice's Fix opens the same form the Doctor row's Fix opens: the keys already set shown
 * locked, a refusal under its field, and, once the fix is applied, the doctor checks again and
 * the Notice goes because its finding has.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";

function row(name: string, status: DoctorRow["status"], over: Partial<DoctorRow> = {}): DoctorRow {
  return {
    name,
    status,
    detail: `${name} said`,
    hint: `${name} hint`,
    checked: true,
    settings: null,
    fix: null,
    ...over,
  };
}

const unset = row("git identity", "fail", {
  detail: "not set: user.email",
  fix: "git-identity",
});
const set = row("git identity", "ok", { detail: "Bea Terminal <bea@example.invalid>" });

const report = (rows: DoctorRow[]): DoctorReport => ({
  rows,
  app_rows: [],
  full: false,
  path: "/usr/bin:/bin",
});

type Asked = { cmd: string; args: unknown };

/** The core: one project with one chat, and a doctor whose identity is `identity()`. */
function core(answers: {
  identity: () => DoctorRow;
  fixIdentity?: (args: Record<string, unknown>) => unknown;
}): Asked[] {
  const asked: Asked[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    const given = (args ?? {}) as Record<string, unknown>;
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        workspaces: [
          {
            name: "alpha",
            path: `${PLANE}/workspaces/alpha`,
            vision: "",
            todos: [],
            chats: [],
          },
        ],
        personas: ["steward"],
        persona: "steward",
        unfiled: [],
      };
    if (cmd === "plane_doctor") return report([row("git", "ok"), answers.identity()]);
    if (cmd === "plane_doctor_identity") return { name: "Bea Terminal", email: "" };
    if (cmd === "plane_doctor_fix_identity") return answers.fixIdentity?.(given) ?? null;
    if (cmd === "opened_chats" || cmd === "chat_states" || cmd === "running_sessions") return [];
    if (cmd === "chats_that_would_not_start") return [];
    return null;
  });
  return asked;
}

beforeEach(() => forgetThisLaunch());

afterEach(() => {
  cleanup();
  clearMocks();
});

const identityNotice = async () =>
  (await screen.findByText(/^git identity: not set: user\.email/)).closest(
    "[data-cause]",
  ) as HTMLElement;

/** What the window last asked the core to keep dismissed for this project. */
const kept = (asked: Asked[]) =>
  (asked.filter((one) => one.cmd === "set_dismissed").at(-1)?.args as { causes?: string[] })
    ?.causes;

/** Lets the window finish opening: where the keyboard starts is placed once it has. */
const settle = () => new Promise((done) => setTimeout(done, 100));

/**
 * Types into a field the form has put the keyboard in. Not by a click: jsdom lays nothing out,
 * so every point is on the resize handles, and a click anywhere moves the focus to one.
 */
async function type(field: HTMLElement, text: string) {
  await waitFor(() => expect(field).toHaveFocus());
  await userEvent.type(field, text, { skipClick: true });
}

async function openForm() {
  const notice = await identityNotice();
  await settle();
  const fix = within(notice).getByRole("button", { name: "Fix" });
  await userEvent.click(fix);
  const form = await screen.findByRole("form", { name: "Git identity" });
  return { notice, fix, form };
}

describe("the doctor's git identity finding, as a Notice (#1250)", () => {
  it("stands under the strip with its fix id, and its Fix opens the form, set keys locked", async () => {
    const asked = core({ identity: () => unset });
    render(<App />);
    expect(await identityNotice()).toHaveAttribute("data-cause", "doctor-finding:git-identity");

    const { notice: line, fix, form } = await openForm();
    // A fix that takes input is not applied by the press alone.
    expect(asked.map((one) => one.cmd)).not.toContain("plane_doctor_fix");
    expect(asked.map((one) => one.cmd)).not.toContain("plane_doctor_fix_identity");
    expect(fix).toHaveAttribute("aria-expanded", "true");
    expect(fix).toHaveAttribute("aria-controls", form.id);
    // Beside the line, never inside its live region: typing is not read out as the Notice.
    expect(line).toHaveAttribute("role", "status");
    expect(line.contains(form)).toBe(false);
    expect(form.closest('[role="status"], [aria-live]')).toBeNull();

    const name = within(form).getByLabelText("Name");
    await waitFor(() => expect(name).toHaveValue("Bea Terminal"));
    expect(name).toBeDisabled();
    expect(name).toHaveAccessibleDescription(/Already set/);
    await waitFor(() => expect(within(form).getByLabelText("Email")).toHaveFocus());
  });

  it("shows the core's refusal under the field it is about, and keeps the Notice", async () => {
    core({
      identity: () => unset,
      fixIdentity: () => ({
        kind: "invalid",
        name: [],
        email: ["That does not look like an email."],
      }),
    });
    render(<App />);
    const { notice, form } = await openForm();

    const email = within(form).getByLabelText("Email");
    await type(email, "nope");
    await userEvent.click(within(form).getByRole("button", { name: "Set identity" }));

    await waitFor(() => expect(email).toHaveAccessibleDescription(/does not look like an email/));
    expect(email).toHaveValue("nope");
    expect(notice).toBeInTheDocument();
  });

  it("goes once the fix is applied and the doctor checks again", async () => {
    let fixed = false;
    const asked = core({
      identity: () => (fixed ? set : unset),
      fixIdentity: () => {
        fixed = true;
        return {
          kind: "fixed",
          fixed: {
            fix: "git-identity",
            refused: null,
            said: ["✓ set user.email = bea@example.invalid (global git config)"],
            complete: true,
          },
        };
      },
    });
    render(<App />);
    const { form } = await openForm();

    await type(within(form).getByLabelText("Email"), "bea@example.invalid");
    await userEvent.click(within(form).getByRole("button", { name: "Set identity" }));

    expect(asked).toContainEqual({
      cmd: "plane_doctor_fix_identity",
      args: { plane: PLANE, name: "", email: "bea@example.invalid" },
    });
    await waitFor(() => expect(screen.queryByText(/^git identity: not set/)).toBeNull());
    // Gone because the doctor answered again without the finding, not because it was hidden.
    expect(asked.filter((one) => one.cmd === "plane_doctor").length).toBeGreaterThan(1);
  });

  it("says a fix refused as a whole on the Notice, which stays", async () => {
    core({
      identity: () => unset,
      fixIdentity: () => ({
        kind: "fixed",
        fixed: {
          fix: "git-identity",
          refused: "your global git config picks identities by folder (includeIf)",
          said: [],
          complete: false,
        },
      }),
    });
    render(<App />);
    const { notice, form } = await openForm();
    await type(within(form).getByLabelText("Email"), "bea@example.invalid");
    await userEvent.click(within(form).getByRole("button", { name: "Set identity" }));

    expect(await within(notice).findByText(/picks identities by folder/)).toBeVisible();
    expect(screen.queryByRole("form", { name: "Git identity" })).toBeNull();
  });

  it("stays dismissed until the finding is mended, and shows again when it comes back (V91j)", async () => {
    let identity = unset;
    const asked = core({ identity: () => identity });
    render(<App />);
    const notice = await identityNotice();
    await settle();
    await userEvent.click(within(notice).getByRole("button", { name: "Dismiss" }));

    expect(screen.queryByText(/^git identity: not set/)).toBeNull();
    await waitFor(() => expect(kept(asked)).toEqual(["doctor-finding:git-identity"]));

    /** Asks the doctor again, as the operator does by opening it. */
    const checkAgain = async () => {
      const before = asked.filter((one) => one.cmd === "plane_doctor").length;
      await userEvent.click(screen.getByTestId("status-doctor"));
      const dialog = await screen.findByRole("dialog");
      await waitFor(() =>
        expect(asked.filter((one) => one.cmd === "plane_doctor").length).toBeGreaterThan(before),
      );
      await settle();
      await userEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    };

    // Still found: still dismissed.
    await checkAgain();
    expect(screen.queryByText(/^git identity: not set/)).toBeNull();
    expect(kept(asked)).toEqual(["doctor-finding:git-identity"]);

    // Mended (set from a terminal): the dismissal is let go.
    identity = set;
    await checkAgain();
    await waitFor(() => expect(kept(asked)).toEqual([]));

    // And it comes back: the Notice shows again.
    identity = unset;
    await checkAgain();
    expect(await identityNotice()).toBeInTheDocument();
  });

  it("is not drawn while the identity is set", async () => {
    core({ identity: () => set });
    render(<App />);
    await screen.findByRole("tablist", { name: "Workspaces" });
    await new Promise((done) => setTimeout(done, 50));

    expect(document.querySelector('[data-cause^="doctor-finding:"]')).toBeNull();
  });
});
