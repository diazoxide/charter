import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { PlaneId } from "./bindings";
import { forgetRepoClones } from "./repoClones";
import { useWorkspaceRepos } from "./WorkspaceRepos";

/**
 * **Settings › Repos' ticks that are not applied yet** (#1192): held for the workspace, not in
 * the group, so switching to another group of Settings and back finds them as they were. The
 * whole Settings tab's Repos row is `settings/WorkspaceLevel.test.tsx`'s.
 */

afterEach(() => {
  cleanup();
  clearMocks();
  forgetRepoClones();
});

const PLANE = "/projects/alpha" as unknown as PlaneId;

/** The core: `cloned` is what is on disk, which a clone adds to. */
function core(cloned: string[]) {
  const asked: string[] = [];
  mockIPC((cmd, args) => {
    asked.push(cmd);
    const given = (args ?? {}) as Record<string, unknown>;
    if (cmd === "reachable_repos")
      return {
        repos: ["api", "web", "docs"].map((name) => ({
          name,
          path: `acme/${name}`,
          description: "",
        })),
        trouble: [],
      };
    if (cmd === "workspace_repos")
      return {
        workspace: given.workspace,
        repos: cloned.map((name) => ({ name })),
        cache_refused: null,
      };
    if (cmd === "take_repos") return null;
    if (cmd === "clone_repo") {
      cloned.push(String(given.repo));
      return [`Cloned ${String(given.repo)}.`];
    }
    if (cmd === "workspace_panels")
      return { workspace: given.workspace, repos: [...cloned], paths: {}, absent: [] };
    return null;
  });
  return asked;
}

/** The Repos group's row, as Settings draws it at the workspace's level. */
function Repos({ workspace = "web-app" }: { workspace?: string }) {
  const { control } = useWorkspaceRepos(PLANE, workspace);
  return <>{control({ id: "repos", labelledBy: "repos-label" })}</>;
}

const box = (name: string) => screen.findByRole("checkbox", { name });

describe("Settings › Repos' unapplied ticks", () => {
  it("survive a switch to another group and back", async () => {
    core(["api"]);
    const view = render(<Repos />);
    await waitFor(async () => expect(await box("api")).toBeChecked());

    await userEvent.click(await box("web"));
    await userEvent.click(await box("api"));
    expect(screen.getByRole("button", { name: "Clone 1, remove 1" })).toBeEnabled();

    // Another group of Settings: the Repos group is unmounted, then drawn again.
    view.unmount();
    render(<Repos />);

    await waitFor(async () => expect(await box("web")).toBeChecked());
    expect(await box("api")).not.toBeChecked();
    expect(await box("docs")).not.toBeChecked();
    expect(screen.getByRole("button", { name: "Clone 1, remove 1" })).toBeEnabled();
  });

  it("are held per workspace", async () => {
    core(["api"]);
    const view = render(<Repos />);
    await waitFor(async () => expect(await box("api")).toBeChecked());
    await userEvent.click(await box("web"));
    view.unmount();

    render(<Repos workspace="other" />);

    await waitFor(async () => expect(await box("api")).toBeChecked());
    expect(await box("web")).not.toBeChecked();
    expect(screen.getByRole("button", { name: "No changes" })).toBeDisabled();
  });

  it("are let go of once applied, and the next drawing is what is on disk", async () => {
    const cloned = ["api"];
    core(cloned);
    const view = render(<Repos />);
    await waitFor(async () => expect(await box("api")).toBeChecked());
    await userEvent.click(await box("web"));

    await userEvent.click(screen.getByRole("button", { name: "Clone 1" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "No changes" })).toBeDisabled());
    expect(cloned).toEqual(["api", "web"]);
    view.unmount();

    // Off disk, unticked by hand: a tick held from before Apply would draw it ticked still.
    cloned.splice(cloned.indexOf("web"), 1);
    render(<Repos />);
    await waitFor(async () => expect(await box("api")).toBeChecked());
    expect(await box("web")).not.toBeChecked();
    expect(screen.getByRole("button", { name: "No changes" })).toBeDisabled();
  });

  it("drop a tick the disk already agrees with when it is read again", async () => {
    const cloned = ["api"];
    core(cloned);
    const view = render(<Repos />);
    await waitFor(async () => expect(await box("api")).toBeChecked());
    await userEvent.click(await box("web"));
    view.unmount();

    // Cloned meanwhile from somewhere else, and then removed there again: no change is left
    // to apply, and none comes back.
    cloned.push("web");
    const again = render(<Repos />);
    await waitFor(() => expect(screen.getByRole("button", { name: "No changes" })).toBeDisabled());
    again.unmount();
    cloned.splice(cloned.indexOf("web"), 1);
    render(<Repos />);

    await waitFor(async () => expect(await box("api")).toBeChecked());
    expect(await box("web")).not.toBeChecked();
    expect(within(screen.getByTestId("repo-picker")).getAllByRole("checkbox")).toHaveLength(3);
  });
});
