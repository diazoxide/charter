import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { Sidebar } from "./Sidebar";
import type { Sidebar as SidebarModel } from "./bindings";

afterEach(cleanup);

const model: SidebarModel = {
  root: "/home/dev/plane",
  personas: ["devops", "steward"],
  persona: "steward",
  unfiled: [],
  workspaces: [
    {
      name: "alpha",
      path: "/home/dev/plane/workspaces/alpha",
      vision: "Ship the widget",
      todos: ["Review the rollout plan", "Write the migration"],
      chats: [
        { session: 1, cwd: "/home/dev/plane/workspaces/alpha" },
        { session: 2, cwd: "/home/dev/plane/workspaces/alpha/svc" },
      ],
    },
    {
      name: "beta",
      path: "/home/dev/plane/workspaces/beta",
      vision: "Retire the old importer",
      todos: [],
      chats: [],
    },
  ],
};

const names = () => screen.getAllByRole("tab").map((el) => el.textContent);

describe("Sidebar", () => {
  it("lists every workspace on the plane", () => {
    render(<Sidebar sidebar={model} focused="alpha" onFocus={() => {}} />);

    expect(names()).toEqual(["alpha", "beta"]);
  });

  it("shows each workspace's chats under it", () => {
    render(<Sidebar sidebar={model} focused="alpha" onFocus={() => {}} />);

    const alpha = screen.getByTestId("workspace-alpha");
    expect(alpha).toHaveTextContent("session 1");
    expect(alpha).toHaveTextContent("session 2");
    expect(screen.getByTestId("workspace-beta")).toHaveTextContent("No chats");
  });

  it("shows the focused workspace's todos and the persona a chat would adopt", () => {
    render(<Sidebar sidebar={model} focused="alpha" onFocus={() => {}} />);

    const focused = screen.getByTestId("focused");
    expect(focused).toHaveTextContent("Review the rollout plan");
    expect(focused).toHaveTextContent("Write the migration");
    expect(focused).toHaveTextContent("steward");
  });

  it("shows only the focused workspace's todos, not another's", () => {
    render(<Sidebar sidebar={model} focused="beta" onFocus={() => {}} />);

    const focused = screen.getByTestId("focused");
    expect(focused).not.toHaveTextContent("Review the rollout plan");
    expect(focused).toHaveTextContent("Nothing to do");
  });

  it("says a workspace has no vision rather than leaving the line blank", () => {
    const unset: SidebarModel = {
      ...model,
      workspaces: [
        {
          name: "gamma",
          path: "/home/dev/plane/workspaces/gamma",
          vision: "",
          todos: [],
          chats: [],
        },
      ],
    };

    render(<Sidebar sidebar={unset} focused="gamma" onFocus={() => {}} />);

    expect(screen.getByTestId("workspace-gamma")).toHaveTextContent("No vision yet");
  });

  it("focuses the workspace that was clicked", async () => {
    const focused: string[] = [];
    render(<Sidebar sidebar={model} focused="alpha" onFocus={(name) => focused.push(name)} />);

    await userEvent.click(screen.getByRole("tab", { name: "beta" }));

    expect(focused).toEqual(["beta"]);
  });

  it("shows a chat working outside every workspace rather than dropping it", () => {
    const stray: SidebarModel = {
      ...model,
      unfiled: [{ session: 9, cwd: "/tmp" }],
    };

    render(<Sidebar sidebar={stray} focused="alpha" onFocus={() => {}} />);

    expect(screen.getByTestId("unfiled")).toHaveTextContent("session 9");
  });
});
