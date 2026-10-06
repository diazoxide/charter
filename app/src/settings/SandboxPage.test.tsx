import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SettingsTab } from "./SettingsTab";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";
import { forgetGroups } from "./links";
import { whatAChatCanDo } from "./sandbox";
import { namedIn } from "./project";
import type {
  SandboxPreset,
  SandboxState,
  SettingsEdit,
  SettingsField,
  SettingsFile,
  SettingsWhich,
} from "../bindings";

/**
 * **Settings › Project › Sandbox explains what a chat here can do** (#1340): one sentence at the
 * top, Internet access as the core's presets, each opening to its hosts, a status line in place
 * of the mode, and what chats can change and what is always protected, each with why. Every
 * preset and every host is the core's (`sandbox_state`): the presets here stand in for its table.
 */

const PLANE = "/home/dev/plane";

const AI: SandboxPreset = {
  word: "model-providers",
  title: "AI providers",
  hosts: ["api.anthropic.com", "api.openai.com"],
};
const CODE: SandboxPreset = {
  word: "forge",
  title: "Code hosting",
  hosts: ["github.com", "gitlab.com", "git.example.org"],
};
const PACKAGES: SandboxPreset = {
  word: "toolchains",
  title: "Package registries",
  hosts: ["registry.npmjs.org", "crates.io"],
};
const PRESETS = [AI, CODE, PACKAGES];

const ON = {
  on: true,
  projectHosts: 0,
  yourHosts: 0,
  folders: 0,
  certificateService: false,
};

describe("the sentence at the top", () => {
  const changes = "A chat here can change files in the folder it works in";
  const caches = `${changes} and in the project's package caches`;
  it.each<[string, readonly SandboxPreset[], string]>([
    ["none", [], `${changes}, and reach no host on the internet.`],
    ["AI providers", [AI], `${changes}, and reach AI providers.`],
    ["code hosting", [CODE], `${changes}, and reach code hosting.`],
    ["package registries", [PACKAGES], `${caches}, and reach package registries.`],
    [
      "AI providers and code hosting",
      [AI, CODE],
      `${changes}, and reach AI providers and code hosting.`,
    ],
    [
      "AI providers and package registries",
      [AI, PACKAGES],
      `${caches}, and reach AI providers and package registries.`,
    ],
    [
      "code hosting and package registries",
      [CODE, PACKAGES],
      `${caches}, and reach code hosting and package registries.`,
    ],
    [
      "every preset",
      PRESETS,
      `${caches}, and reach AI providers, code hosting and package registries.`,
    ],
  ])("says what a chat can do with %s on", (_, presets, said) => {
    expect(whatAChatCanDo({ ...ON, presets })).toBe(said);
  });

  it("counts the project's hosts and yours, and the certificate service once it is on", () => {
    expect(
      whatAChatCanDo({
        ...ON,
        presets: [AI],
        projectHosts: 2,
        yourHosts: 1,
        certificateService: true,
      }),
    ).toBe(
      "A chat here can change files in the folder it works in, and reach AI providers, 2 project hosts, 1 host of yours and the system's certificate service.",
    );
    expect(whatAChatCanDo({ ...ON, presets: [], projectHosts: 1, yourHosts: 2 })).toBe(
      "A chat here can change files in the folder it works in, and reach 1 project host and 2 hosts of yours.",
    );
  });

  it("counts the folders you allowed every chat here on this machine", () => {
    expect(whatAChatCanDo({ ...ON, presets: [PACKAGES], folders: 2 })).toBe(
      "A chat here can change files in the folder it works in, in the project's package caches and in 2 folders you allowed on this machine, and reach package registries.",
    );
    expect(whatAChatCanDo({ ...ON, presets: [], folders: 1 })).toBe(
      "A chat here can change files in the folder it works in and in 1 folder you allowed on this machine, and reach no host on the internet.",
    );
  });

  it("says a project without the sandbox limits nothing", () => {
    expect(whatAChatCanDo({ ...ON, on: false, presets: PRESETS })).toBe(
      "Chats here run without a sandbox, so a chat can change any file you can and reach any host.",
    );
  });
});

describe("the files' names in the window", () => {
  const PURLIS = { shared: "purlis.toml", local: "purlis.local.toml" };

  it("renames a file's name, and leaves a host or path that only holds it as written", () => {
    expect(
      namedIn(
        "charter.toml names charter.toml.example.com; /opt/mycharter.toml and x-charter.local.toml stay. See charter.local.toml.",
        PURLIS,
      ),
    ).toBe(
      "purlis.toml names charter.toml.example.com; /opt/mycharter.toml and x-charter.local.toml stay. See purlis.local.toml.",
    );
    expect(namedIn("/home/dev/plane/charter.toml, which your team sees", PURLIS)).toBe(
      "/home/dev/plane/purlis.toml, which your team sees",
    );
  });
});

/** What a field of the Shared file holds. */
function field(path: string[], value: SettingsField["value"]): SettingsField {
  return { path: path.map((key) => ({ key })), value };
}

/** A file as the core lists it: its fields, and its hosts as the `hosts` collection. */
function fileOf(
  which: SettingsWhich,
  name: string,
  fields: SettingsField[],
  hosts: readonly string[] = [],
): SettingsFile {
  return {
    which,
    file: name,
    exists: true,
    text: "schema = 1\n",
    refusals: [],
    parsed: true,
    fields,
    entries: hosts.map((host, at) => ({
      collection: which === "shared" ? "hosts" : "myHosts",
      id: `host:${at}:${host}`,
      label: host,
      keys: [{ key: "sandbox" }, { key: "hosts" }],
      values: [
        { field: "host", value: host },
        ...(which === "local" ? [{ field: "confirmed", value: "yes" }] : []),
      ],
    })),
  };
}

const MODE_ON = field(["sandbox", "mode"], { kind: "text", value: "on" });

function state(more: Partial<SandboxState> = {}): SandboxState {
  return {
    on: true,
    offer: false,
    said: "no chat has started under this project's sandbox on this machine yet",
    never: [],
    hosts_changed: null,
    presets: PRESETS,
    persona_hosts: [],
    besides: { project_hosts: 0, your_hosts: 0, folders: 0 },
    ...more,
  };
}

/** The core, as the Sandbox page asks it: the two files and the sandbox's state. */
function core({
  shared = [MODE_ON],
  sharedName = "charter.toml",
  localName = "charter.local.toml",
  sandbox = state(),
  hosts = [] as string[],
}: {
  shared?: SettingsField[];
  sharedName?: string;
  localName?: string;
  sandbox?: SandboxState | null;
  hosts?: string[];
} = {}) {
  const sent: { which: SettingsWhich; edits: SettingsEdit[] }[] = [];
  const files: Record<SettingsWhich, SettingsFile> = {
    shared: fileOf("shared", sharedName, shared, hosts),
    local: fileOf("local", localName, []),
  };
  mockIPC(
    (cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "project_settings":
          return files;
        case "project_extensions":
          return { extensions: [], local_left_out: null };
        case "project_harness_plugins":
        case "extensions_on":
        case "sandbox_grants":
          return [];
        case "grantable_folders":
          return { folders: [], dropped: [] };
        case "project_theme":
        case "project_theme_drawn":
          return null;
        case "sandbox_state":
          return sandbox;
        case "save_project_settings": {
          const which = given.which as SettingsWhich;
          const change = given.change as { kind: "edits"; edits: SettingsEdit[] };
          sent.push({ which, edits: change.edits });
          const kept = files[which].fields.filter(
            (one) =>
              !change.edits.some((edit) => JSON.stringify(edit.path) === JSON.stringify(one.path)),
          );
          const added = change.edits.flatMap((edit) =>
            edit.value === null ? [] : [{ path: edit.path, value: edit.value }],
          );
          files[which] = { ...files[which], fields: [...kept, ...added] };
          return { kind: "saved", file: files[which] };
        }
      }
      return undefined;
    },
    { shouldMockEvents: true },
  );
  return { sent };
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

async function atSandbox(): Promise<HTMLElement> {
  render(<SettingsTab plane={PLANE} level="project" />);
  await userEvent.click(
    await within(await screen.findByRole("navigation", { name: "Groups" })).findByRole("button", {
      name: "Sandbox",
    }),
  );
  return screen.findByRole("region", { name: "Sandbox" });
}

describe("Settings › Project › Sandbox", () => {
  it("opens on the sentence, and it follows a preset as it is turned off", async () => {
    const { sent } = core();
    const page = await atSandbox();

    await waitFor(() =>
      expect(page).toHaveTextContent(
        "A chat here can change files in the folder it works in and in the project's package caches, and reach AI providers, code hosting and package registries.",
      ),
    );

    await userEvent.click(within(page).getByRole("checkbox", { name: "Package registries" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].edits).toEqual([
      {
        path: [{ key: "sandbox" }, { key: "egress" }],
        value: { kind: "list", value: ["model-providers", "forge"] },
      },
    ]);
    await waitFor(() =>
      expect(page).toHaveTextContent(
        "A chat here can change files in the folder it works in, and reach AI providers and code hosting.",
      ),
    );
  });

  it("ticks the presets the file names, and writes none as no host rather than every preset", async () => {
    const { sent } = core({
      shared: [MODE_ON, field(["sandbox", "egress"], { kind: "list", value: ["forge"] })],
    });
    const page = await atSandbox();
    const access = await within(page).findByRole("group", { name: "Internet access" });

    expect(within(access).getByRole("checkbox", { name: "AI providers" })).not.toBeChecked();
    expect(within(access).getByRole("checkbox", { name: "Code hosting" })).toBeChecked();
    expect(within(access).getByRole("checkbox", { name: "Package registries" })).not.toBeChecked();

    await userEvent.click(within(access).getByRole("checkbox", { name: "Code hosting" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].edits[0].value).toEqual({ kind: "list", value: [] });
  });

  it("lists each preset's hosts as the core gives them, behind a disclosure", async () => {
    core();
    const page = await atSandbox();
    const access = await within(page).findByRole("group", { name: "Internet access" });

    const code = within(access).getByText("3 hosts", { selector: "summary" });
    const hosts = code.closest("details");
    expect(hosts).not.toBeNull();
    expect(
      within(hosts as HTMLElement)
        .getAllByRole("listitem")
        .map((one) => one.textContent),
    ).toEqual(["github.com", "gitlab.com", "git.example.org"]);
  });

  it("shows the mode as a status, with how one chat runs without it", async () => {
    core();
    const page = await atSandbox();

    const status = await within(page).findByRole("group", { name: "Sandbox" });
    expect(status).toHaveTextContent(
      "On for everyone in this project. To run one chat without it, use that chat's tab.",
    );
    expect(within(page).queryByRole("combobox", { name: "Sandbox mode" })).toBeNull();
    expect(within(page).queryByRole("button", { name: "Reset" })).toBeNull();
  });

  it("turns the sandbox on from its status line, with no way back from here", async () => {
    const { sent } = core({ shared: [], sandbox: state({ on: false, said: null }) });
    const page = await atSandbox();

    expect(page).toHaveTextContent(
      "Chats here run without a sandbox, so a chat can change any file you can and reach any host.",
    );
    await userEvent.click(within(page).getByRole("button", { name: "Turn the sandbox on" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].edits).toEqual([
      { path: [{ key: "sandbox" }, { key: "mode" }], value: { kind: "text", value: "on" } },
    ]);
    await waitFor(() =>
      expect(within(page).getByRole("group", { name: "Sandbox" })).toHaveTextContent(
        "On for everyone in this project.",
      ),
    );
    expect(within(page).queryByRole("button", { name: "Undo" })).toBeNull();
    expect(within(page).queryByRole("button", { name: "Turn the sandbox on" })).toBeNull();
  });

  it("writes certificate checks as true or false", async () => {
    const { sent } = core();
    const page = await atSandbox();

    await userEvent.selectOptions(within(page).getByLabelText("Certificate checks"), "on");

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].edits).toEqual([
      {
        path: [{ key: "sandbox" }, { key: "certificate-checks" }],
        value: { kind: "bool", value: true },
      },
    ]);
  });

  it("names a persona's own hosts", async () => {
    core({
      sandbox: state({
        persona_hosts: [
          { persona: "devops", hosts: ["10.0.0.5:6443", "charter.toml.example.com"] },
        ],
      }),
      sharedName: "purlis.toml",
      localName: "purlis.local.toml",
    });
    const page = await atSandbox();

    await waitFor(() =>
      expect(page).toHaveTextContent(
        "A chat as devops also reaches 10.0.0.5:6443 and charter.toml.example.com.",
      ),
    );
    expect(page).toHaveTextContent(
      "A chat that names no persona reaches its default persona's hosts. A chat opened by a handoff may hold the asking chat's hosts, and a Resume the default persona's, until you allow its own on its tab.",
    );
  });

  it("says a chat can read what you can, so no login of yours reads as hidden", async () => {
    core();
    const page = await atSandbox();

    await waitFor(() =>
      expect(page).toHaveTextContent(
        "A chat can read any file you can, except vaults and purlis's own keys, so keep secrets in a vault.",
      ),
    );
    const kept = await within(page).findByRole("list", { name: "Always protected" });
    expect(kept).toHaveTextContent("Your approvals in purlis");
    expect(kept).not.toHaveTextContent(/sign-in/i);
  });

  it("counts what the core grants: the project's hosts, yours and your folders", async () => {
    core({
      hosts: ["api.example.com", "10.0.0.5"],
      sandbox: state({ besides: { project_hosts: 1, your_hosts: 0, folders: 1 } }),
    });
    const page = await atSandbox();

    await waitFor(() =>
      expect(page).toHaveTextContent(
        "A chat here can change files in the folder it works in, in the project's package caches and in 1 folder you allowed on this machine, and reach AI providers, code hosting, package registries and 1 project host.",
      ),
    );
  });

  it("says it is reading, not that a chat reaches nothing, until the core answers", async () => {
    core({ sandbox: null });
    const page = await atSandbox();

    await waitFor(() => expect(page).toHaveTextContent("Reading this project's sandbox…"));
    expect(page).not.toHaveTextContent("no host on the internet");
  });

  it("names the certificate service only on a Mac, where the setting widens anything", async () => {
    core({
      shared: [MODE_ON, field(["sandbox", "certificate-checks"], { kind: "bool", value: true })],
    });
    const page = await atSandbox();

    await waitFor(() => expect(page).toHaveTextContent("A chat here can change files"));
    expect(page).not.toHaveTextContent("certificate service");
  });

  it("is found by searching for what it explains", async () => {
    core();
    render(<SettingsTab plane={PLANE} level="project" />);
    await userEvent.type(await screen.findByRole("searchbox"), "Always protected");

    const page = await screen.findByRole("region", { name: "Sandbox" });
    expect(within(page).getByRole("list", { name: "Always protected" })).toBeInTheDocument();
  });

  it("lists what chats can change and what is always protected, each with why", async () => {
    core();
    const page = await atSandbox();

    const changes = await within(page).findByRole("list", { name: "What chats can change" });
    expect(within(changes).getAllByRole("listitem").length).toBeGreaterThan(3);
    expect(changes).toHaveTextContent("The project's package caches");
    const kept = within(page).getByRole("list", { name: "Always protected" });
    expect(kept).toHaveTextContent("Vaults");
    expect(kept).toHaveTextContent("charter.toml and charter.local.toml");
    for (const item of within(kept).getAllByRole("listitem"))
      expect(item.querySelector(".sandbox-why")?.textContent).toMatch(/^[A-Z].+\.$/);
  });

  it("leaves the package caches off what chats can change while Package registries is off", async () => {
    core({ shared: [MODE_ON, field(["sandbox", "egress"], { kind: "list", value: ["forge"] })] });
    const page = await atSandbox();

    const changes = await within(page).findByRole("list", { name: "What chats can change" });
    expect(changes).not.toHaveTextContent("package caches");
  });

  it("names the files the project uses, under their purlis names too", async () => {
    core({ sharedName: "purlis.toml", localName: "purlis.local.toml" });
    const page = await atSandbox();

    await waitFor(() => expect(page).toHaveTextContent("Kept in purlis.toml"));
    expect(page).toHaveTextContent("purlis.toml and purlis.local.toml");
    expect(page).not.toHaveTextContent("charter.toml");
  });

  it("names the files the project uses on every Project group", async () => {
    core({ sharedName: "purlis.toml", localName: "purlis.local.toml" });
    render(<SettingsTab plane={PLANE} level="project" />);
    const nav = await screen.findByRole("navigation", { name: "Groups" });
    for (const name of ["Forges", "Saving", "General"]) {
      await userEvent.click(await within(nav).findByRole("button", { name }));
      const group = await screen.findByRole("region", { name });
      expect(group, name).not.toHaveTextContent("charter.toml");
      expect(group, name).not.toHaveTextContent("charter.local.toml");
    }
  });

  it("starts no sentence on the page with a lowercase letter", async () => {
    core();
    const page = await atSandbox();
    await within(page).findByRole("list", { name: "Always protected" });

    // Each piece of text the page draws, by the element that holds it. A host or a file name is
    // in code font and an option of a select is a value: neither is a sentence.
    const texts = [...page.querySelectorAll("p, span, label, li, summary, h3, button, div")]
      .filter((one) => one.closest("code, option") === null)
      .map((one) =>
        [...one.childNodes]
          .filter((node) => node.nodeType === Node.TEXT_NODE)
          .map((node) => node.textContent)
          .join("")
          .trim(),
      )
      .filter((one) => one.length > 0);
    expect(texts.length).toBeGreaterThan(10);
    const sentences = texts.flatMap((text) => text.split(/(?<=\.)\s+/));
    for (const sentence of sentences) expect(sentence, sentence).toMatch(/^[^a-z]/);
  });
});
