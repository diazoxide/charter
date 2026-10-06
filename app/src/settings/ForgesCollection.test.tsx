import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SettingsTab } from "./SettingsTab";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";
import { forgetGroups } from "./links";
import type {
  EntryReferrer,
  EntryWritten,
  ForgeEntry,
  SettingsField,
  SettingsFile,
} from "../bindings";

/**
 * **Settings › Forges as a collection** (ST-3, #1227): Add opens an inline form of setting rows,
 * each `[[forge]]` block has Remove, a written add or remove has the one-level Undo — the
 * inverse operation through the core — and what the core refuses is said where it belongs.
 *
 * The core is a model here, as small as the window's contract with it: the blocks, the text
 * they are written as, an identity per block that changes once it moves, and a write refused
 * when the text it is sent against is not the text on disk. What it checks beyond that is
 * `purlis_core::settings::forges`'s tests; held here is what the person sees and what the
 * window sends.
 */

const PLANE = "/home/dev/plane";

type Block = { kind: string; owner: string; host: string };

const text = (blocks: readonly Block[]) =>
  `schema = 1\n${blocks.map((one) => `\n[[forge]]\nkind = "${one.kind}"\nowner = "${one.owner}"\n${one.host ? `host = "${one.host}"\n` : ""}`).join("")}`;

const idOf = (blocks: readonly Block[], at: number) =>
  `forge:${at}:${blocks[at].kind}-${blocks[at].owner}`;

const labelOf = (one: Block, at: number) =>
  `Forge ${at + 1}: ${one.kind} ${one.owner} at ${one.host || `${one.kind}.com`}`;

const field = (keys: (string | number)[], value: string): SettingsField => ({
  path: keys.map((one) => (typeof one === "number" ? { index: one } : { key: one })),
  value: { kind: "text", value },
});

/** `charter.toml` as the core answers it for `blocks`. */
function fileOf(blocks: readonly Block[]): SettingsFile {
  return {
    which: "shared",
    file: "charter.toml",
    exists: true,
    text: text(blocks),
    refusals: [],
    parsed: true,
    fields: [
      field(["schema"], "1"),
      ...blocks.flatMap((one, at) => [
        field(["forge", at, "kind"], one.kind),
        field(["forge", at, "owner"], one.owner),
      ]),
    ],
    entries: blocks.map((one, at) => ({
      collection: "forges",
      id: idOf(blocks, at),
      label: labelOf(one, at),
      keys: [{ key: "forge" }, { index: at }],
      values: [
        { field: "kind", value: one.kind },
        { field: "owner", value: one.owner },
        { field: "host", value: one.host },
        { field: "exclude", value: "" },
      ],
    })),
  };
}

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

const ACME: Block = { kind: "github", owner: "acme", host: "" };
const BETA: Block = { kind: "github", owner: "beta", host: "" };

type Asked =
  | { cmd: "add"; base: string | null; entry: ForgeEntry }
  | { cmd: "remove"; base: string | null; id: string }
  | { cmd: "raw"; base: string | null; text: string };

const MOVED = "charter.toml changed on disk since this tab read it, so nothing was saved.";

const refusal = (
  over: Partial<{
    fields: { field: string; why: string }[];
    referrers: EntryReferrer[];
    reasons: string[];
  }>,
): EntryWritten => ({ kind: "refused", fields: [], referrers: [], reasons: [], ...over });

/**
 * The core, as a model of the window's contract with it. `refuseAdd` / `refuseRemove` answer
 * with a refusal instead of writing; `holdAdds` makes each add wait for `release()`.
 */
function core({
  blocks: start = [ACME],
  refuseAdd,
  refuseRemove,
  holdRaw = false,
}: {
  blocks?: Block[];
  refuseAdd?: (entry: ForgeEntry) => EntryWritten | undefined;
  refuseRemove?: (id: string) => EntryWritten | undefined;
  /** Each whole-text write waits for `release()`. */
  holdRaw?: boolean;
} = {}) {
  let blocks = [...start];
  /** Every text the file has been, and the blocks it was: what a whole-text write puts back. */
  const seen = new Map<string, Block[]>([[text(blocks), blocks]]);
  const asked: Asked[] = [];
  const held: (() => void)[] = [];
  mockIPC(
    async (cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "project_settings":
          return { shared: fileOf(blocks), local: LOCAL };
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
          const base = given.base as string | null;
          asked.push({ cmd: "add", base, entry });
          if (base !== text(blocks)) return refusal({ reasons: [MOVED] });
          const answer = refuseAdd?.(entry);
          if (answer) return answer;
          blocks = [...blocks, { kind: entry.kind, owner: entry.owner, host: entry.host }];
          seen.set(text(blocks), blocks);
          return {
            kind: "saved",
            file: fileOf(blocks),
            added: idOf(blocks, blocks.length - 1),
            removed: null,
          };
        }
        case "remove_project_forge": {
          const id = given.id as string;
          const base = given.base as string | null;
          asked.push({ cmd: "remove", base, id });
          if (base !== text(blocks)) return refusal({ reasons: [MOVED] });
          const at = blocks.findIndex((_, place) => idOf(blocks, place) === id);
          if (at < 0)
            return refusal({ reasons: ["That forge is not in charter.toml as it was shown"] });
          const answer = refuseRemove?.(id);
          if (answer) return answer;
          const took = blocks[at];
          blocks = blocks.filter((_, place) => place !== at);
          seen.set(text(blocks), blocks);
          return {
            kind: "saved",
            file: fileOf(blocks),
            added: null,
            removed: [
              { field: "kind", value: took.kind },
              { field: "owner", value: took.owner },
              { field: "host", value: took.host },
              { field: "exclude", value: "" },
            ],
          };
        }
        case "save_project_settings": {
          const change = given.change as { kind: "raw"; text: string };
          const base = given.base as string | null;
          asked.push({ cmd: "raw", base, text: change.text });
          if (holdRaw) await new Promise<void>((go) => held.push(go));
          if (base !== text(blocks)) return { kind: "refused", reasons: [MOVED] };
          blocks = seen.get(change.text) ?? blocks;
          return { kind: "saved", file: fileOf(blocks) };
        }
      }
      return undefined;
    },
    { shouldMockEvents: true },
  );
  return {
    asked,
    owners: () => blocks.map((one) => one.owner),
    /** Lets the oldest held write be answered. */
    release: () => act(async () => held.shift()?.()),
  };
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

const nav = () => screen.getByRole("navigation", { name: "Groups" });
const block = (name: string | RegExp) => screen.findByRole("group", { name });
const removeOf = (label: string) => screen.getByRole("button", { name: `Remove ${label}` });

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
    core({ blocks: [] });
    render(<SettingsTab plane={PLANE} level="project" />);
    await screen.findByRole("button", { name: "General" });
    expect(within(nav()).getByRole("button", { name: "Forges" })).toBeInTheDocument();
  });

  it("heads each block with the core's label, over its own rows, with its Remove", async () => {
    core();
    await atForges();
    const one = await block("Forge 1: github acme at github.com");
    expect(within(one).getByLabelText("Forge 1: owner")).toHaveValue("acme");
    expect(removeOf("Forge 1: github acme at github.com")).toBeInTheDocument();
  });

  it("adds a forge from an inline form of rows, sent whole against the text it was drawn from", async () => {
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
      base: text([ACME]),
      entry: {
        kind: "gitlab",
        owner: "platform",
        host: "git.acme.dev",
        exclude: ["sandbox", "old"],
      },
    });
    expect(await block("Forge 2: gitlab platform at git.acme.dev")).toBeInTheDocument();
    expect(screen.queryByRole("form", { name: "New forge" })).not.toBeInTheDocument();
    expect(screen.getByText("Added Forge 2: gitlab platform at git.acme.dev.")).toBeInTheDocument();
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
      refuseAdd: () =>
        refusal({ fields: [{ field: "host", why: "host 'https://x/' is not a hostname" }] }),
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

  it("removes a block by its identity, and undoes it by writing the text it took it from back", async () => {
    const { asked, owners } = core();
    await atForges();

    await userEvent.click(removeOf("Forge 1: github acme at github.com"));
    await waitFor(() => expect(owners()).toEqual([]));
    expect(asked[0]).toEqual({ cmd: "remove", base: text([ACME]), id: idOf([ACME], 0) });
    expect(screen.getByText("Removed Forge 1: github acme at github.com.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add forge" })).toHaveFocus();

    await userEvent.click(screen.getByRole("button", { name: "Undo" }));

    await waitFor(() => expect(owners()).toEqual(["acme"]));
    // Exact: the text the entry was removed from, against the text the remove left.
    expect(asked[1]).toEqual({ cmd: "raw", base: text([]), text: text([ACME]) });
    expect(await block("Forge 1: github acme at github.com")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
  });

  it("undoes an add by removing that entry, refused with its users when one came since", async () => {
    const { asked, owners } = core({
      refuseRemove: (id) =>
        id === idOf([ACME, { kind: "gitlab", owner: "ops", host: "" }], 1)
          ? refusal({
              referrers: [
                { what: "The repo tools (inventory/repos.json) is on gitlab.com.", group: null },
              ],
            })
          : undefined,
    });
    await atForges();
    await userEvent.click(screen.getByRole("button", { name: "Add forge" }));
    await userEvent.type(screen.getByLabelText("Owner"), "ops");
    await userEvent.click(
      within(screen.getByRole("form", { name: "New forge" })).getByRole("button", {
        name: "Add forge",
      }),
    );
    await waitFor(() => expect(owners()).toEqual(["acme", "ops"]));

    await userEvent.click(screen.getByRole("button", { name: "Undo" }));

    const alert = await screen.findByRole("alert");
    expect(asked[1]).toEqual({
      cmd: "remove",
      base: text([ACME, { kind: "gitlab", owner: "ops", host: "" }]),
      id: idOf([ACME, { kind: "gitlab", owner: "ops", host: "" }], 1),
    });
    expect(alert).toHaveTextContent("This forge is not removed while this uses it:");
    expect(alert).toHaveTextContent("The repo tools (inventory/repos.json) is on gitlab.com.");
    expect(owners()).toEqual(["acme", "ops"]);
  });

  it("sends a Remove against the text it was drawn from, so one queued behind an Undo takes nothing else", async () => {
    // The review's sequence: [acme, beta]; Remove acme; Undo, slow; Remove beta, now Forge 1.
    const { asked, owners, release } = core({ blocks: [ACME, BETA], holdRaw: true });
    await atForges();
    await userEvent.click(removeOf("Forge 1: github acme at github.com"));
    await waitFor(() => expect(owners()).toEqual(["beta"]));
    const drawn = text([BETA]);

    await userEvent.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(asked).toHaveLength(2));
    await userEvent.click(removeOf("Forge 1: github beta at github.com"));
    await release();

    await waitFor(() => expect(asked).toHaveLength(3));
    expect(asked[2]).toEqual({ cmd: "remove", base: drawn, id: idOf([BETA], 0) });
    // acme came back where it was, first, and nothing else went: the stale Remove was refused.
    expect(asked[1]).toEqual({ cmd: "raw", base: drawn, text: text([ACME, BETA]) });
    await waitFor(() => expect(owners()).toEqual(["acme", "beta"]));
    expect(await screen.findByText(MOVED)).toBeInTheDocument();
  });

  it("names what uses a block it does not remove, by that block, with a link to where each is changed", async () => {
    core({
      blocks: [ACME, BETA],
      refuseRemove: () =>
        refusal({
          referrers: [
            { what: "The repo billing (inventory/repos.json) is on github.com.", group: null },
            {
              what: '[repos.billing] mode = "pr" in charter.toml opens a request on github.com.',
              group: "project.saving",
            },
          ],
        }),
    });
    await atForges();

    await userEvent.click(removeOf("Forge 2: github beta at github.com"));

    const alert = await screen.findByRole("alert");
    expect(within(await block("Forge 2: github beta at github.com")).getByRole("alert")).toBe(
      alert,
    );
    expect(alert).toHaveTextContent("This forge is not removed while these use it:");
    expect(within(alert).getAllByRole("button", { name: "Fix it in Settings" })).toHaveLength(1);

    await userEvent.click(within(alert).getByRole("button", { name: "Fix it in Settings" }));
    expect(within(nav()).getByRole("button", { name: "Saving" })).toHaveAttribute(
      "aria-current",
      "true",
    );
  });

  it("draws no link for a referrer whose group this tab cannot open", async () => {
    core({
      refuseRemove: () =>
        refusal({
          referrers: [
            { what: "The workspace ide saves billing as a request.", group: "workspace.saving" },
          ],
        }),
    });
    await atForges();
    await userEvent.click(removeOf("Forge 1: github acme at github.com"));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("The workspace ide saves billing as a request.");
    expect(within(alert).queryByRole("button")).not.toBeInTheDocument();
  });

  it("clears a refused Remove once anything else is written", async () => {
    core({
      refuseRemove: () =>
        refusal({ referrers: [{ what: "The repo billing is on github.com.", group: null }] }),
    });
    await atForges();
    await userEvent.click(removeOf("Forge 1: github acme at github.com"));
    await screen.findByRole("alert");

    await userEvent.click(screen.getByRole("button", { name: "Add forge" }));
    await userEvent.type(screen.getByLabelText("Owner"), "ops");
    await userEvent.click(
      within(screen.getByRole("form", { name: "New forge" })).getByRole("button", {
        name: "Add forge",
      }),
    );

    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  });
});
