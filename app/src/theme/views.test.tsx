/**
 * **The token test, run on every view** (DS-1): each of charter's own views and an extension's,
 * drawn in each built-in theme, holds no colour but a token's.
 *
 * `literals.test.ts` reads the source and refuses a colour written into it. That is the half
 * of the rule a reader can check; this is the half only a drawing can: what reaches the DOM.
 * A colour that arrives at run time — an inline style built from a constant, an SVG attribute
 * a library writes, a `var(--x)` spelled from a string — is in no source line the literal
 * guard reads, and is exactly what a theme could not change.
 *
 * So every view is rendered as a tab draws it, from the core's answers, and every element of
 * it is held to three things:
 *
 * - **an inline style carries no colour literal**, and every custom property it reads is a
 *   token, a motion token, or one the window sets for itself;
 * - **every token it reads is set by the theme in force**, so it resolves in both themes rather
 *   than in the one somebody happened to look at;
 * - **an SVG paint attribute is a token, `currentColor` or nothing** — Lucide's icons are
 *   `stroke="currentColor"`, and a library that wrote `fill="#000"` would be the black band of
 *   charter-app#193 again, through a door the stylesheet guard does not watch.
 *
 * A class carries no colour of its own: Tailwind's palette is deleted (`tailwind.test.ts`) and
 * `App.css` holds no literal (`literals.test.ts`), so the class half is already held at the
 * source. An arbitrary-value class (`bg-[#fff]`) is refused here too, because a class built
 * from a string at run time is the one spelling the source guard cannot see.
 */

/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ViewPane } from "../Views";
import type {
  HarnessPlugins,
  InstructionFile,
  MemoryView,
  PlaneSaving,
  ProjectSettings,
  ProjectTheme,
  RepoSaving,
  SavingInForce,
  SessionRecordView,
  SettingsFile,
  VaultContents,
  ViewAnswer,
  WorkspaceSettings,
} from "../bindings";
import type { ViewRef } from "../tabs";
import { MOTION_TOKENS, motionProperty } from "./motion";
import { BUILT_IN, DEFAULT_THEME, TOKENS, drawIn, property } from "./theme";

afterEach(() => {
  cleanup();
  clearMocks();
});

// ---------------------------------------------------------------------------------------------
// What a drawn element may say about colour.
// ---------------------------------------------------------------------------------------------

/** A colour written out: hex, a colour function, or one of CSS's named colours. The same
 *  three `literals.test.ts` refuses in the source. */
const LITERAL =
  /#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color|color-mix)\s*\(|(?<![\w-])(?:white|black|red|green|blue|yellow|orange|purple|pink|brown|gray|grey|silver|gold|cyan|magenta|teal|navy|olive|maroon|lime|aqua|fuchsia)(?![\w-])/;

/** The custom properties the window sets for itself rather than a theme: lengths, never a
 *  colour. The same list, for the same reasons, as `literals.test.ts`'s `fromTheWindow`. */
const FROM_THE_WINDOW = new Set(["--least", "--root", "--chip", "--window-controls"]);

/** The custom properties `App.css` declares for itself — a length or a count, since a colour
 *  there would fail `literals.test.ts`. */
const OWN_PROPERTIES = new Set(
  [
    ...readFileSync(join(process.cwd(), "src/App.css"), "utf8").matchAll(/^\s+(--[a-z0-9-]+):/gm),
  ].map((hit) => hit[1]),
);

const COLOUR_TOKENS = new Set(TOKENS.map(property));
const MOTION = new Set(MOTION_TOKENS.map(motionProperty));

/** The SVG attributes that paint. */
const PAINT = ["fill", "stroke", "color", "stop-color", "flood-color", "lighting-color"];

/** A paint value that is no colour of charter's own: no paint, the inherited one, a gradient
 *  or pattern by reference, or a token. */
const PAINT_OK = /^(?:none|currentColor|transparent|inherit|url\(#[\w-]+\)|var\(--[a-z0-9-]+\))$/i;

/** Tailwind's arbitrary-value spelling, which no theme can reach. */
const ARBITRARY = /^[a-z][a-z0-9-]*-\[[^\]\s]+\]$/;

/**
 * Everything about `root`'s drawing that a theme could not change, one sentence each;
 * empty when every colour in it is a token the theme in force sets.
 */
function complaints(root: Element): string[] {
  const said: string[] = [];
  const set = document.documentElement.style;
  for (const element of [root, ...root.querySelectorAll("*")]) {
    const where = describeElement(element);
    const style = (element as HTMLElement | SVGElement).style;
    if (style) {
      for (let at = 0; at < style.length; at += 1) {
        const name = style[at];
        const value = style.getPropertyValue(name);
        // A custom property's own name is not a colour, but its value can be one: the window
        // setting `--x: #fff` is the same defect as `color: #fff`.
        if (LITERAL.test(value)) said.push(`${where} style ${name}: ${value} is a colour literal`);
        for (const hit of value.matchAll(/var\(\s*(--[a-z0-9-]+)/g)) {
          const read = hit[1];
          if (COLOUR_TOKENS.has(read)) {
            if (set.getPropertyValue(read) === "")
              said.push(`${where} style ${name} reads ${read}, which the theme does not set`);
          } else if (!MOTION.has(read) && !FROM_THE_WINDOW.has(read) && !OWN_PROPERTIES.has(read)) {
            said.push(`${where} style ${name} reads ${read}, which is not a token`);
          }
        }
      }
    }
    if (element instanceof SVGElement) {
      for (const attribute of PAINT) {
        const value = element.getAttribute(attribute);
        if (value !== null && !PAINT_OK.test(value.trim()))
          said.push(`${where} ${attribute}="${value}" is not a token`);
      }
    }
    for (const name of element.classList) {
      if (ARBITRARY.test(name)) said.push(`${where} class ${name} is an arbitrary value`);
    }
  }
  return said;
}

/** An element said the way a complaint can be found by: its tag and its first class. */
function describeElement(element: Element): string {
  const first = element.classList[0];
  return first === undefined
    ? `<${element.tagName.toLowerCase()}>`
    : `<${element.tagName.toLowerCase()}.${first}>`;
}

describe("the token test", () => {
  beforeEach(() => drawIn(DEFAULT_THEME));

  /** One element, as markup, checked on its own. */
  function check(markup: string): string[] {
    const host = document.createElement("div");
    host.innerHTML = markup;
    document.body.append(host);
    try {
      return complaints(host);
    } finally {
      host.remove();
    }
  }

  it("passes an element drawn from tokens", () => {
    expect(
      check(
        `<p style="color: var(--text-muted); inline-size: 40%"><svg stroke="currentColor" fill="none"></svg></p>`,
      ),
    ).toEqual([]);
  });

  it("refuses a colour written into an inline style", () => {
    expect(check(`<p style="color: #ff0000"></p>`)).not.toEqual([]);
    expect(check(`<p style="background: white"></p>`)).not.toEqual([]);
    expect(check(`<p style="border-color: rgb(1, 2, 3)"></p>`)).not.toEqual([]);
  });

  it("refuses a custom property set to a colour", () => {
    expect(check(`<p style="--anything: #123456"></p>`)).toHaveLength(1);
  });

  it("refuses a custom property that is not a token", () => {
    expect(check(`<p style="color: var(--text-mutd)"></p>`)).toEqual([
      "<p> style color reads --text-mutd, which is not a token",
    ]);
  });

  it("refuses a token the theme in force does not set", () => {
    document.documentElement.style.removeProperty("--text-muted");
    expect(check(`<p style="color: var(--text-muted)"></p>`)).toEqual([
      "<p> style color reads --text-muted, which the theme does not set",
    ]);
  });

  it("refuses an SVG painted with a colour of its own", () => {
    expect(check(`<svg><path fill="#000"></path></svg>`)).toEqual([
      '<path> fill="#000" is not a token',
    ]);
  });

  it("refuses an arbitrary-value class", () => {
    expect(check(`<p class="bg-[#fff]"></p>`)).toEqual([
      "<p.bg-[#fff]> class bg-[#fff] is an arbitrary value",
    ]);
  });
});

// ---------------------------------------------------------------------------------------------
// Every view, as the core answers for it.
// ---------------------------------------------------------------------------------------------

const PLANE = "/home/dev/plane";
const WORKSPACE = "web";

/** A panel answer that uses every block the vocabulary has, so every way a view draws one is
 *  drawn: a note in each tone, facts, a list with a row and a chart. */
const ANSWERED: ViewAnswer = {
  kind: "answered",
  blocks: [
    { kind: "note", text: "The steward", tone: "default" },
    { kind: "note", text: "Something is off", tone: "warn" },
    { kind: "facts", facts: [{ label: "Vault", value: "none" }] },
    {
      kind: "list",
      rows: [
        {
          key: "a",
          text: "Charter defects go upstream",
          note: "2026-09-20",
          mark: "note",
          tone: "plain",
          detail: { kind: "text", text: "File the issue." },
          runs: null,
          actions: [],
        },
      ],
      empty: { headline: "Nothing remembered yet", body: null, offer: null },
    },
    {
      kind: "chart",
      title: "Memories per persona",
      shape: "bars",
      unit: "memories",
      points: [
        { label: "steward", value: 3, note: "75%" },
        { label: "devops", value: 1, note: null },
      ],
    },
  ],
  took_ms: 2,
  overreach: null,
};

const SHARED_FILE: SettingsFile = {
  which: "shared",
  file: "charter.toml",
  exists: true,
  text: '[memory]\nshare = "local"\n',
  refusals: [],
  parsed: true,
  fields: [
    { path: [{ key: "memory" }, { key: "share" }], value: { kind: "text", value: "local" } },
  ],
};

const SETTINGS: ProjectSettings = {
  shared: SHARED_FILE,
  local: {
    ...SHARED_FILE,
    which: "local",
    file: "charter.local.toml",
    exists: false,
    text: "",
    fields: [],
  },
};

const THEME: ProjectTheme = {
  options: [
    { value: "charter-dark", label: "charter-dark (built in)" },
    { value: "charter-light", label: "charter-light (built in)" },
  ],
  picked: null,
  file: null,
  draws: null,
  why: null,
  colour: null,
  ignored: [],
  local_left_out: null,
};

const said = (value: string | null, source = "default") => ({ value, source });

const SAVING_IN_FORCE: SavingInForce = {
  plane: {
    mode: said(null),
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

const HARNESSES: HarnessPlugins[] = [
  {
    harness: "claude",
    title: "Claude Code",
    unsupported: null,
    record: "~/.claude/plugins/installed_plugins.json",
    trouble: null,
    plugins: [],
    local_left_out: null,
  },
];

const VAULT: VaultContents = {
  name: "ops",
  provider: "keyring",
  count: 1,
  health: { ok: true, detail: "reachable" },
  secrets: [{ key: "DB_URL", size: "24 B", updated: "2026-09-20" }],
  identity: [],
  identity_in_app_env: [],
};

const PLANE_SAVING: PlaneSaving = {
  stage: "changed",
  changed: ["workspaces/web/workspace.md"],
  ahead: 0,
  pr: null,
  blocked: null,
  branch: "main",
  pushes: true,
  behind: 0,
  pushFailed: null,
  live: [WORKSPACE],
  conflicts: [],
  notice: null,
  mode: null,
  modeFrom: "default",
  journal: [],
};

const REPO_SAVING: RepoSaving[] = [
  {
    name: "site",
    mode: "off",
    modeFrom: "default",
    autosave: false,
    stage: "off",
    branch: "main",
    changed: 0,
    ahead: null,
    pr: null,
    blocked: null,
    pushes: false,
    target: null,
    ownBranch: false,
  },
];

const RECORD: SessionRecordView = {
  row: {
    path: "workspaces/web/sessions/2026-09-20-a.md",
    title: "Tokens for every view",
    when: "2026-09-20 10:00",
    persona: null,
    harness: "claude",
    resumable: true,
  },
  place: WORKSPACE,
  body: "# Tokens for every view\n\n## What happened\n\nThe guard ran.\n",
};

const MEMORY: MemoryView = {
  scope: { kind: "shared" },
  slug: "grill",
  title: "Grill back hard",
  stamp: "2026-09-21 09:00",
  place: "shared",
  body: "Recommend, **don't** offer menus.\n",
  path: "memory/grill.md",
  text: "# Grill back hard\n\nRecommend, **don't** offer menus.\n",
};

const WORKSPACE_FILE: WorkspaceSettings = {
  workspace: WORKSPACE,
  file: "workspaces/web/workspace.json",
  exists: true,
  text: '{ "settings": { "theme": "charter-dark" } }',
  refusals: [],
  parsed: true,
  fields: [{ path: [{ key: "theme" }], value: { kind: "text", value: "charter-dark" } }],
  live: true,
};

const INSTRUCTIONS: InstructionFile[] = [
  {
    repo: "site",
    file: "AGENTS.md",
    text: "# Agents\n\nRun the tests.\n",
    standing: { kind: "offered", caution: null },
  },
];

/** The core, answering every question a view asks with the answers above. */
function core() {
  mockIPC((cmd) => {
    switch (cmd) {
      case "open_view":
        return ANSWERED;
      case "vault_open":
        return VAULT;
      case "project_settings":
        return SETTINGS;
      case "project_extensions":
        return { extensions: [], local_left_out: null };
      case "project_harness_plugins":
        return HARNESSES;
      case "project_saving_in_force":
        return SAVING_IN_FORCE;
      case "project_theme":
        return THEME;
      case "project_theme_drawn":
        return null;
      case "plane_saving":
        return PLANE_SAVING;
      case "workspace_saving":
        return REPO_SAVING;
      case "session_record":
        return RECORD;
      case "memory_read":
        return MEMORY;
      case "workspace_settings":
        return WORKSPACE_FILE;
      case "repo_instructions":
        return INSTRUCTIONS;
      default:
        return null;
    }
  });
}

/** Every view a tab can show, and words that are on it once its answer is drawn — the wait
 *  that makes this a test of the view and not of its "opening" line. */
const VIEWS: { name: string; view: ViewRef; drawn: RegExp }[] = [
  {
    name: "a persona",
    view: { from: null, view: "persona", key: "steward" },
    drawn: /Charter defects go upstream/,
  },
  { name: "a vault", view: { from: null, view: "vault", key: "ops" }, drawn: /DB_URL/ },
  {
    name: "Project settings",
    view: { from: null, view: "settings", key: "" },
    drawn: /charter\.toml/,
  },
  { name: "Saving", view: { from: null, view: "saving", key: "" }, drawn: /site/ },
  {
    name: "a workspace's changes",
    view: { from: null, view: "changes", key: WORKSPACE },
    drawn: /Charter defects go upstream/,
  },
  {
    name: "a session record",
    view: { from: null, view: "session", key: RECORD.row.path },
    drawn: /The guard ran/,
  },
  {
    name: "a memory",
    view: { from: null, view: "memory", key: "shared/grill" },
    drawn: /offer menus/,
  },
  {
    name: "Shared memory",
    view: { from: null, view: "shared-memory", key: "" },
    drawn: /Charter defects go upstream/,
  },
  {
    name: "a workspace's settings",
    view: { from: null, view: "workspace-settings", key: WORKSPACE },
    drawn: /workspace\.json/,
  },
  {
    name: "a workspace's repo instructions",
    view: { from: null, view: "repo-instructions", key: WORKSPACE },
    drawn: /AGENTS\.md/,
  },
  { name: "Preferences", view: { from: null, view: "preferences", key: "" }, drawn: /^Text$/ },
  {
    name: "an extension's view",
    view: { from: "persona-statistics", view: "statistics", key: "" },
    drawn: /Memories per persona/,
  },
];

describe.each(Object.keys(BUILT_IN))("every view in %s", (theme) => {
  beforeEach(() => drawIn(BUILT_IN[theme]));
  afterEach(() => drawIn(DEFAULT_THEME));

  it.each(VIEWS)("draws $name with tokens only", async ({ view, drawn }) => {
    core();
    const { container } = render(
      <ViewPane
        plane={PLANE}
        view={view}
        title="The view"
        workspace={WORKSPACE}
        waits={false}
        offered={[]}
        onOpenView={() => {}}
        onAsk={() => {}}
        onVaultChanged={() => {}}
      />,
    );
    expect((await screen.findAllByText(drawn)).length).toBeGreaterThan(0);
    expect(complaints(container)).toEqual([]);
  });
});
