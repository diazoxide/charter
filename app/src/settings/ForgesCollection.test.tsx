import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SettingsTab } from "./SettingsTab";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";
import type { EntryWritten, ForgeEntry, SettingsField, SettingsFile } from "../bindings";

/**
 * **Settings › Forges as a collection** (ST-3, #1227): Add opens an inline form of setting rows,
 * each `[[forge]]` block has Remove, a written add or remove has the one-level Undo, and what the
 * core refuses is said where it belongs. The core is the mock here — what it checks and what it
 * writes is `charter_core::settings::forges`'s tests; held here is what the person sees and what
 * the window sends.
 */

const PLANE = "/home/dev/plane";

const ONE_FORGE = `schema = 1\n\n[[forge]]\nkind = "github"\nowner = "acme"\n`;

const field = (keys: (string | number)[], value: string): SettingsField => ({
  path: keys.map((one) => (typeof one === "number" ? { index: one } : { key: one })),
  value: { kind: "text", value },
});

const file = (text: string, fields: SettingsField[]): SettingsFile => ({
  which: "shared",
  file: "charter.toml",
  exists: true,
  text,
  refusals: [],
  parsed: true,
  fields,
});

const SHARED = file(ONE_FORGE, [
  field(["schema"], "1"),
  field(["forge", 0, "kind"], "github"),
  field(["forge", 0, "owner"], "acme"),
]);

const NONE = file("schema = 1\n", [field(["schema"], "1")]);

const LOCAL: SettingsFile = {
  which: "local",
  file: "charter.local.toml",
  exists: false,
  text: "",
  refusals: [],
  parsed: true,
  fields: [],
};

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

type Asked =
  | { cmd: "add"; base: string | null; entry: ForgeEntry }
  | { cmd: "remove"; base: string | null; index: number }
  | { cmd: "raw"; base: string | null; text: string };

/** The core, as a mock: `add` and `remove` answer with what they are handed, or write. */
function core({
  shared = SHARED,
  add,
  remove,
}: {
  shared?: SettingsFile;
  add?: (entry: ForgeEntry) => EntryWritten;
  remove?: (index: number) => EntryWritten;
} = {}) {
  const files = { shared, local: LOCAL };
  const asked: Asked[] = [];
  mockIPC(
    (cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "project_settings":
          // A copy, as the wire hands one: the window never holds the core's own object.
          return { ...files };
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
          return { on: false, offer: false, said: null, never: [] };
        case "add_project_forge": {
          const entry = given.entry as ForgeEntry;
          asked.push({ cmd: "add", base: given.base as string | null, entry });
          const answer = add?.(entry);
          if (answer) return answer;
          const at = new Set(
            files.shared.fields
              .filter((one) => one.path[0]?.key === "forge")
              .map((one) => JSON.stringify(one.path[1])),
          ).size;
          files.shared = file(`${files.shared.text}\n[[forge]]\nkind = "${entry.kind}"\n`, [
            ...files.shared.fields,
            field(["forge", at, "kind"], entry.kind),
            field(["forge", at, "owner"], entry.owner),
          ]);
          return { kind: "saved", file: files.shared };
        }
        case "remove_project_forge": {
          const index = given.index as number;
          asked.push({ cmd: "remove", base: given.base as string | null, index });
          const answer = remove?.(index);
          if (answer) return answer;
          files.shared = NONE;
          return { kind: "saved", file: files.shared };
        }
        case "save_project_settings": {
          const change = given.change as { kind: "raw"; text: string };
          asked.push({ cmd: "raw", base: given.base as string | null, text: change.text });
          files.shared = change.text === ONE_FORGE ? SHARED : file(change.text, []);
          return { kind: "saved", file: files.shared };
        }
      }
      return undefined;
    },
    { shouldMockEvents: true },
  );
  return { asked };
}

beforeEach(() => {
  forgetThisLaunch();
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

const nav = () => screen.getByRole("navigation", { name: "Groups" });

async function atForges() {
  render(<SettingsTab plane={PLANE} level="project" />);
  await userEvent.click(
    await within(await screen.findByRole("navigation", { name: "Groups" })).findByRole("button", {
      name: "Forges",
    }),
  );
}

describe("Settings › Forges", () => {
  it("is offered while the project declares no forge, so one can be added", async () => {
    core({ shared: NONE });
    render(<SettingsTab plane={PLANE} level="project" />);
    await screen.findByRole("button", { name: "General" });
    expect(within(nav()).getByRole("button", { name: "Forges" })).toBeInTheDocument();
  });

  it("heads each block with what it is, over its own rows, with its Remove", async () => {
    core();
    await atForges();
    const block = screen.getByRole("group", { name: "Forge 1: github acme at github.com" });
    expect(within(block).getByLabelText("Forge 1: owner")).toHaveValue("acme");
    expect(
      within(block).getByRole("button", { name: "Remove Forge 1: github acme at github.com" }),
    ).toBeInTheDocument();
  });

  it("adds a forge from an inline form of rows, sent whole against the text it read", async () => {
    const { asked } = core();
    await atForges();

    await userEvent.click(screen.getByRole("button", { name: "Add forge" }));
    const form = screen.getByRole("form", { name: "New forge" });
    expect(within(form).getByLabelText("Kind")).toHaveValue("gitlab");
    await userEvent.type(within(form).getByLabelText("Owner"), "platform");
    await userEvent.type(within(form).getByLabelText("Host"), "git.acme.dev");
    await userEvent.type(within(form).getByLabelText("Repos never listed"), "sandbox{Enter}old");
    await userEvent.click(within(form).getByRole("button", { name: "Add forge" }));

    await waitFor(() => expect(asked).toHaveLength(1));
    expect(asked[0]).toEqual({
      cmd: "add",
      base: ONE_FORGE,
      entry: {
        kind: "gitlab",
        owner: "platform",
        host: "git.acme.dev",
        exclude: ["sandbox", "old"],
      },
    });
    expect(
      await screen.findByRole("group", { name: "Forge 2: gitlab platform at gitlab.com" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("form", { name: "New forge" })).not.toBeInTheDocument();
    expect(screen.getByText("Added forge gitlab platform.")).toBeInTheDocument();
  });

  it("opens Add on its first field, and Escape closes it with the focus back on Add", async () => {
    const { asked } = core();
    await atForges();
    await userEvent.click(screen.getByRole("button", { name: "Add forge" }));
    expect(screen.getByLabelText("Kind")).toHaveFocus();

    await userEvent.keyboard("{Escape}");

    expect(screen.queryByRole("form", { name: "New forge" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add forge" })).toHaveFocus();
    expect(asked).toHaveLength(0);
  });

  it("says a refused field under that field, and keeps what was typed", async () => {
    core({
      add: () => ({
        kind: "refused",
        fields: [{ field: "host", why: "host 'https://x/' is not a hostname" }],
        referrers: [],
        reasons: [],
      }),
    });
    await atForges();
    await userEvent.click(screen.getByRole("button", { name: "Add forge" }));
    const form = screen.getByRole("form", { name: "New forge" });
    await userEvent.type(within(form).getByLabelText("Host"), "https://x/");
    await userEvent.click(within(form).getByRole("button", { name: "Add forge" }));

    const why = await within(form).findByText("host 'https://x/' is not a hostname");
    const row = why.closest(".ui-setting-row") as HTMLElement;
    expect(within(row).getByLabelText("Host")).toHaveValue("https://x/");
    expect(within(form).getByLabelText("Host")).toHaveAccessibleDescription(
      expect.stringContaining("is not a hostname"),
    );
  });

  it("removes a block and undoes it by putting the file's text back", async () => {
    const { asked } = core();
    await atForges();

    await userEvent.click(
      screen.getByRole("button", { name: "Remove Forge 1: github acme at github.com" }),
    );
    await waitFor(() => expect(asked).toHaveLength(1));
    expect(asked[0]).toEqual({ cmd: "remove", base: ONE_FORGE, index: 0 });
    await waitFor(() =>
      expect(screen.queryByRole("group", { name: /^Forge 1/ })).not.toBeInTheDocument(),
    );
    expect(screen.getByText("Removed Forge 1: github acme at github.com.")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Undo" }));

    await waitFor(() => expect(asked).toHaveLength(2));
    expect(asked[1]).toEqual({ cmd: "raw", base: "schema = 1\n", text: ONE_FORGE });
    expect(
      await screen.findByRole("group", { name: "Forge 1: github acme at github.com" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
  });

  it("names what uses a block it does not remove, with a link to where each is changed", async () => {
    core({
      remove: () => ({
        kind: "refused",
        fields: [],
        referrers: [
          { what: "The repo billing (inventory/repos.json) is on git.acme.dev.", group: null },
          {
            what: '[repos.billing] mode = "pr" in charter.toml opens a request on git.acme.dev.',
            group: "project.saving",
          },
        ],
        reasons: [],
      }),
    });
    await atForges();

    await userEvent.click(
      screen.getByRole("button", { name: "Remove Forge 1: github acme at github.com" }),
    );

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("This forge is not removed while these use it:");
    expect(alert).toHaveTextContent("The repo billing (inventory/repos.json) is on git.acme.dev.");
    expect(within(alert).getAllByRole("button", { name: "Fix it in Settings" })).toHaveLength(1);
    expect(screen.getByRole("group", { name: /^Forge 1/ })).toBeInTheDocument();

    await userEvent.click(within(alert).getByRole("button", { name: "Fix it in Settings" }));
    expect(within(nav()).getByRole("button", { name: "Saving" })).toHaveAttribute(
      "aria-current",
      "true",
    );
  });
});
