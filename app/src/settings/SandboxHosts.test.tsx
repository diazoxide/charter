import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SettingsTab } from "./SettingsTab";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";
import { forgetGroups } from "./links";
import type { EntryWritten, SettingsFile, SettingsWhich } from "../bindings";

/**
 * **Settings › Sandbox's hosts** (#1341): the project's own hosts, kept in `charter.toml` and
 * followed by every teammate, and yours, in `charter.local.toml` on this machine only — each a
 * list with Add and Remove. The core checks every host (`purlis_core::settings::hosts`); held
 * here is what the person sees and what the window sends.
 */

const PLANE = "/home/dev/plane";

const NAMES: Record<SettingsWhich, string> = { shared: "hosts", local: "myHosts" };

function fileOf(
  which: SettingsWhich,
  hosts: readonly string[],
  unconfirmed: readonly string[] = [],
): SettingsFile {
  const text =
    which === "shared"
      ? `schema = 1\n\n[sandbox]\nmode = "on"\nhosts = ${JSON.stringify(hosts)}\n`
      : hosts.length > 0
        ? `[sandbox]\nhosts = ${JSON.stringify(hosts)}\n`
        : "";
  return {
    which,
    file: which === "shared" ? "charter.toml" : "charter.local.toml",
    exists: which === "shared" || hosts.length > 0,
    text,
    refusals: [],
    parsed: true,
    fields:
      which === "shared"
        ? [
            { path: [{ key: "schema" }], value: { kind: "text", value: "1" } },
            { path: [{ key: "sandbox" }, { key: "mode" }], value: { kind: "text", value: "on" } },
          ]
        : [],
    entries: hosts.map((host, at) => ({
      collection: NAMES[which],
      id: `host:${at}:${host}`,
      label: unconfirmed.includes(host) ? `${host} (not yet confirmed)` : host,
      keys: [{ key: "sandbox" }, { key: "hosts" }],
      values: [
        { field: "host", value: host },
        ...(which === "local"
          ? [{ field: "confirmed", value: unconfirmed.includes(host) ? "no" : "yes" }]
          : []),
      ],
    })),
  };
}

const said = (value: string | null, source = "default") => ({ value, source });

const SAVING = {
  plane: {
    mode: said("push", "shared"),
    from_share: false,
    branch: said(null),
    save_branch: said(null),
    sign: said("off"),
    autosave: said("on"),
    autosave_after: said("1m"),
  },
  repos: [],
  plane_left_out: null,
  repos_left_out: null,
};

type Asked = { cmd: string; which: SettingsWhich; base: string | null; arg: string };

function core({
  refuse,
  local = [],
  unconfirmed = [],
}: {
  refuse?: (host: string) => string | undefined;
  local?: string[];
  unconfirmed?: string[];
} = {}) {
  const hosts: Record<SettingsWhich, string[]> = { shared: ["10.100.39.145:6443"], local };
  let waiting = [...unconfirmed];
  const local_ = () => fileOf("local", hosts.local, waiting);
  const asked: Asked[] = [];
  mockIPC(
    (cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      const which = given.which as SettingsWhich;
      switch (cmd) {
        case "project_settings":
          return { shared: fileOf("shared", hosts.shared), local: local_() };
        case "project_extensions":
          return { extensions: [], local_left_out: null };
        case "project_harness_plugins":
          return [];
        case "project_saving_in_force":
          return SAVING;
        case "project_theme":
        case "project_theme_drawn":
          return null;
        case "extensions_on":
          return [];
        case "sandbox_state":
          return {
            on: true,
            offer: false,
            said: null,
            never: [],
            hosts_changed: null,
            presets: [],
            persona_hosts: [],
            besides: { project_hosts: 0, your_hosts: 0, folders: 0 },
          };
        case "add_sandbox_host": {
          const host = given.host as string;
          asked.push({ cmd, which, base: given.base as string | null, arg: host });
          const why = refuse?.(host);
          if (why)
            return {
              kind: "refused",
              fields: [{ field: "host", why }],
              referrers: [],
              reasons: [],
            } satisfies EntryWritten;
          hosts[which] = [...hosts[which], host];
          return {
            kind: "saved",
            file: fileOf(which, hosts[which]),
            added: `host:${hosts[which].length - 1}:${host}`,
            removed: null,
          } satisfies EntryWritten;
        }
        case "confirm_sandbox_host": {
          const id = given.id as string;
          asked.push({ cmd, which: "local", base: given.base as string | null, arg: id });
          const host = hosts.local.find((one, at) => `host:${at}:${one}` === id) ?? "";
          waiting = waiting.filter((one) => one !== host);
          return { kind: "saved", file: local_(), added: id, removed: null } satisfies EntryWritten;
        }
        case "remove_sandbox_host": {
          const id = given.id as string;
          asked.push({ cmd, which, base: given.base as string | null, arg: id });
          const took = hosts[which].find((host, at) => `host:${at}:${host}` === id) ?? "";
          hosts[which] = hosts[which].filter((host) => host !== took);
          return {
            kind: "saved",
            file: fileOf(which, hosts[which]),
            added: null,
            removed: [{ field: "host", value: took }],
          } satisfies EntryWritten;
        }
      }
      return undefined;
    },
    { shouldMockEvents: true },
  );
  return { asked, hosts };
}

beforeEach(() => {
  forgetThisLaunch();
  forgetGroups();
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: "/home/dev/layout.json", found: false, document: null, trouble: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

async function at(group: string) {
  render(<SettingsTab plane={PLANE} level="project" />);
  await userEvent.click(
    await within(await screen.findByRole("navigation", { name: "Groups" })).findByRole("button", {
      name: group,
    }),
  );
}

describe("Settings › Sandbox's hosts", () => {
  it("lists the project's hosts, each with Remove", async () => {
    core();
    await at("Sandbox");
    expect(
      await screen.findByRole("button", { name: "Remove 10.100.39.145:6443" }),
    ).toBeInTheDocument();
  });

  it("adds a project host to the committed file, sent against the text it was drawn from", async () => {
    const { asked, hosts } = core();
    await at("Sandbox");

    await userEvent.click(await screen.findByRole("button", { name: "Add host" }));
    const form = screen.getByRole("form", { name: "New host" });
    await userEvent.type(within(form).getByLabelText("Host"), "*.internal.example");
    await userEvent.click(within(form).getByRole("button", { name: "Add host" }));

    await waitFor(() => expect(hosts.shared).toContain("*.internal.example"));
    expect(asked[0]).toEqual({
      cmd: "add_sandbox_host",
      which: "shared",
      base: fileOf("shared", ["10.100.39.145:6443"]).text,
      arg: "*.internal.example",
    });
    expect(await screen.findByText("Added *.internal.example.")).toBeInTheDocument();
  });

  it("says why a host is refused under its field, and keeps what was typed", async () => {
    core({
      refuse: () => "That is a URL. Type its host alone, such as x.example, without the scheme.",
    });
    await at("Sandbox");
    await userEvent.click(await screen.findByRole("button", { name: "Add host" }));
    const form = screen.getByRole("form", { name: "New host" });
    await userEvent.type(within(form).getByLabelText("Host"), "https://x.example/");
    await userEvent.click(within(form).getByRole("button", { name: "Add host" }));

    expect(await within(form).findByText(/That is a URL/)).toBeInTheDocument();
    expect(within(form).getByLabelText("Host")).toHaveValue("https://x.example/");
  });

  it("removes a project host by its identity", async () => {
    const { asked, hosts } = core();
    await at("Sandbox");
    await userEvent.click(await screen.findByRole("button", { name: "Remove 10.100.39.145:6443" }));
    await waitFor(() => expect(hosts.shared).toEqual([]));
    expect(asked[0]).toMatchObject({
      cmd: "remove_sandbox_host",
      which: "shared",
      arg: "host:0:10.100.39.145:6443",
    });
  });

  it("adds your own host to this machine's file alone, on its own page", async () => {
    const { asked, hosts } = core();
    await at("Your hosts");
    expect(screen.getByText(/on this machine only/)).toBeInTheDocument();

    await userEvent.click(await screen.findByRole("button", { name: "Add host" }));
    const form = screen.getByRole("form", { name: "New host" });
    await userEvent.type(within(form).getByLabelText("Host"), "[[fd00::7]:8443");
    await userEvent.click(within(form).getByRole("button", { name: "Add host" }));

    await waitFor(() => expect(hosts.local).toEqual(["[fd00::7]:8443"]));
    expect(hosts.shared).toEqual(["10.100.39.145:6443"]);
    expect(asked[0]).toMatchObject({ cmd: "add_sandbox_host", which: "local", base: null });
  });

  it("waits for your Confirm on a host Settings did not add here, and confirms it", async () => {
    const { asked } = core({ local: ["10.0.0.6"], unconfirmed: ["10.0.0.6"] });
    await at("Your hosts");

    await userEvent.click(
      await screen.findByRole("button", { name: "Confirm 10.0.0.6 (not yet confirmed)" }),
    );

    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /^Confirm/ })).not.toBeInTheDocument(),
    );
    expect(asked[0]).toMatchObject({ cmd: "confirm_sandbox_host", arg: "host:0:10.0.0.6" });
    expect(screen.getByRole("group", { name: "10.0.0.6" })).toBeInTheDocument();
  });
});
