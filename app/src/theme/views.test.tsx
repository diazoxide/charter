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
 * So every view is rendered as a tab draws it, in every state it has — answered, gone,
 * refused, waiting to be asked, being edited, a row's action refused — from the core's answers,
 * and every element of it is held to four things:
 *
 * - **an inline style carries no colour literal** — hex, a colour function, any of CSS's named
 *   or system colours, or one inside a data URL — and every custom property it reads is a
 *   token, a motion token, or one the window sets for itself;
 * - **every token it reads is set by the theme in force**, so it resolves in both themes rather
 *   than in the one somebody happened to look at;
 * - **an SVG paint attribute is a token, `currentColor` or nothing**, and a token it reads is
 *   held to the two rules above — Lucide's icons are `stroke="currentColor"`, and a library
 *   that wrote `fill="#000"` would be the black band of charter-app#193 again, through a door
 *   the stylesheet guard does not watch;
 * - **no class is an arbitrary value**, in any of Tailwind's spellings: `bg-[#fff]`,
 *   `hover:bg-[#fff]`, `!bg-[#fff]`, `bg-[#fff]/50`, `[color:red]`. A class built from a
 *   string at run time is the one spelling the source guard cannot see.
 *
 * Otherwise a class carries no colour of its own: Tailwind's palette is deleted
 * (`tailwind.test.ts`) and `App.css` holds no literal (`literals.test.ts`), so that half is
 * held at the source.
 *
 * Every view charter has is in `OWN_MARKS`, and a test below fails when one of them has no
 * state here. The window's chrome and its dialogs are #956.
 */

/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { OWN_MARKS, ViewPane } from "../Views";
import type { Offer } from "../actions";
import { DRAFT, forgetDrafts, wantEdit } from "../memories";
import type {
  Activity,
  DispatchRow,
  HarnessPlugins,
  InstructionFile,
  MemoryView,
  PanelRow,
  PlaneSaving,
  ProjectSettings,
  ProjectTheme,
  RepoSaving,
  RowAction,
  SavingInForce,
  SessionRecordView,
  SettingsFile,
  VaultContents,
  ViewAnswer,
  WorkspaceSettings,
} from "../bindings";
import type { ViewRef } from "../tabs";
import { literalsIn } from "./literal";
import { MOTION_TOKENS, motionProperty } from "./motion";
import { BUILT_IN, DEFAULT_THEME, TOKENS, drawIn, inForce, property, tinted } from "./theme";
import { forgetGroups } from "../settings/links";

afterEach(() => {
  cleanup();
  clearMocks();
});

// ---------------------------------------------------------------------------------------------
// What a drawn element may say about colour.
// ---------------------------------------------------------------------------------------------

/** The custom properties the window sets for itself rather than a theme: lengths, never a
 *  colour. The same list, for the same reasons, as `literals.test.ts`'s `fromTheWindow`. */
const FROM_THE_WINDOW = new Set(["--least", "--root", "--chip", "--window-controls"]);

/** The custom properties `App.css` declares for itself — a length or a count, since a colour
 *  there would fail `literals.test.ts`. */
const OWN_PROPERTIES = new Set(
  [...readFileSync(join(process.cwd(), "src/App.css"), "utf8").matchAll(/^\s+(--[\w-]+):/gm)].map(
    (hit) => hit[1],
  ),
);

const COLOUR_TOKENS = new Set(TOKENS.map(property));
const MOTION = new Set(MOTION_TOKENS.map(motionProperty));

/** The SVG attributes that paint. */
const PAINT = ["fill", "stroke", "color", "stop-color", "flood-color", "lighting-color"];

/** A paint value that is no colour of charter's own: no paint, the inherited one, a gradient
 *  or pattern by reference, or a custom property — which is then held to the tokens like any
 *  other `var()`. */
const PAINT_OK = /^(?:none|currentColor|transparent|inherit|url\(#[\w-]+\)|var\(--[\w-]+\))$/i;

/** A custom property read, in any case: `--Text-Muted` is a different property from
 *  `--text-muted`, and a pattern that only saw lower case would let it through unread. */
const READS = /var\(\s*(--[\w-]+)/gi;

/**
 * Tailwind's arbitrary values, anywhere in a class: `bg-[#fff]`, behind a variant
 * (`hover:bg-[#fff]`), with `!` or a `/50` modifier, or a bare arbitrary property
 * (`[color:red]`). No class charter writes has a square bracket in it otherwise, so a bracket
 * at all is the tell — and none of them is a value a theme can reach.
 */
const ARBITRARY = /\[[^\]]*\]/;

/**
 * What is wrong with each `var()` that `value` reads: one that is not a token, or a token the
 * theme in force does not set.
 */
function unread(value: string, said: (what: string) => void): void {
  const set = document.documentElement.style;
  for (const hit of value.matchAll(READS)) {
    const read = hit[1];
    if (COLOUR_TOKENS.has(read)) {
      if (set.getPropertyValue(read) === "") said(`reads ${read}, which the theme does not set`);
    } else if (!MOTION.has(read) && !FROM_THE_WINDOW.has(read) && !OWN_PROPERTIES.has(read)) {
      said(`reads ${read}, which is not a token`);
    }
  }
}

/**
 * Whether `value`, set as the custom property `name` on `element`, is **the theme's own tint**:
 * the token of that name in the theme in force, turned to the hue the element declares in
 * `data-colour` (`theme.tinted`, which is what `tintVariables` writes for a workspace's colour
 * and a persona's, #1449). A theme changes it by changing the token, so it is a token's value
 * and no literal of the window's. Anything else written there is still a literal.
 */
function isTint(element: Element, name: string, value: string): boolean {
  const colour = element.getAttribute("data-colour");
  const token = TOKENS.find((one) => property(one) === name);
  if (colour === null || token === undefined) return false;
  const tint = tinted(inForce(), colour);
  return tint !== inForce() && tint.values[token] === value;
}

/**
 * Everything about `root`'s drawing that a theme could not change, one sentence each;
 * empty when every colour in it is a token the theme in force sets.
 */
function complaints(root: Element): string[] {
  const said: string[] = [];
  for (const element of [root, ...root.querySelectorAll("*")]) {
    const where = describeElement(element);
    const style = (element as HTMLElement | SVGElement).style;
    if (style) {
      for (let at = 0; at < style.length; at += 1) {
        const name = style[at];
        const value = style.getPropertyValue(name);
        // A custom property's own name is not a colour, but its value can be one: the window
        // setting `--x: #fff` is the same defect as `color: #fff`. So is a colour inside an
        // inlined SVG's data URL.
        if (literalsIn(value).length > 0 && !isTint(element, name, value))
          said.push(`${where} style ${name}: ${value} is a colour literal`);
        unread(value, (what) => said.push(`${where} style ${name} ${what}`));
      }
    }
    if (element instanceof SVGElement) {
      for (const attribute of PAINT) {
        const value = element.getAttribute(attribute);
        if (value === null) continue;
        if (!PAINT_OK.test(value.trim()))
          said.push(`${where} ${attribute}="${value}" is not a token`);
        else unread(value, (what) => said.push(`${where} ${attribute}="${value}" ${what}`));
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

  // The loopholes a first version of this checker had, each one probed so it stays shut.

  it("refuses every named CSS colour, not only the common ones", () => {
    expect(check(`<p style="color: rebeccapurple"></p>`)).not.toEqual([]);
    expect(check(`<p style="color: papayawhip"></p>`)).not.toEqual([]);
    expect(check(`<p style="border-color: DarkSlateGray"></p>`)).not.toEqual([]);
  });

  it("refuses a system colour, which is the platform's and not the theme's", () => {
    expect(check(`<p style="color: Canvas"></p>`)).not.toEqual([]);
    expect(check(`<p style="background-color: ButtonFace"></p>`)).not.toEqual([]);
  });

  it("reads a custom property whatever its case, since --Text-Muted is not --text-muted", () => {
    expect(check(`<p style="color: var(--Text-Muted)"></p>`)).toEqual([
      "<p> style color reads --Text-Muted, which is not a token",
    ]);
  });

  it("holds an SVG paint's var() to the tokens and to the theme in force", () => {
    expect(check(`<svg><path fill="var(--text-mutd)"></path></svg>`)).toEqual([
      '<path> fill="var(--text-mutd)" reads --text-mutd, which is not a token',
    ]);
    document.documentElement.style.removeProperty("--accent-base");
    expect(check(`<svg><path stroke="var(--accent-base)"></path></svg>`)).toEqual([
      '<path> stroke="var(--accent-base)" reads --accent-base, which the theme does not set',
    ]);
  });

  it("refuses an arbitrary value behind a variant, an important mark or a modifier", () => {
    for (const name of ["hover:bg-[#fff]", "!bg-[#fff]", "bg-[#fff]/50", "dark:md:text-[red]"]) {
      expect(check(`<p class="${name}"></p>`), name).toEqual([
        `<p.${name}> class ${name} is an arbitrary value`,
      ]);
    }
  });

  it("refuses an arbitrary property", () => {
    expect(check(`<p class="[color:red]"></p>`)).toEqual([
      "<p.[color:red]> class [color:red] is an arbitrary value",
    ]);
  });

  it("refuses a colour inside a data URL, percent-encoded or base64", () => {
    const svg = `<svg xmlns='http://www.w3.org/2000/svg'><path fill='#fff'/></svg>`;
    const encoded = `data:image/svg+xml,${encodeURIComponent(svg).replace(/'/g, "%27")}`;
    const base64 = `data:image/svg+xml;base64,${btoa(svg)}`;
    expect(check(`<p style="background-image: url('${encoded}')"></p>`)).not.toEqual([]);
    expect(check(`<p style="background-image: url('${base64}')"></p>`)).not.toEqual([]);
  });
});

// ---------------------------------------------------------------------------------------------
// Every view, as the core answers for it.
// ---------------------------------------------------------------------------------------------

const PLANE = "/home/dev/plane";
const WORKSPACE = "web";

/** A row of a list, with `over` said on top of a plain one. */
function row(key: string, over: Partial<PanelRow> = {}): PanelRow {
  return {
    key,
    text: `Row ${key}`,
    note: "2026-09-20",
    mark: "dot",
    tone: "plain",
    detail: { kind: "text", text: `About ${key}.` },
    runs: null,
    actions: [],
    ...over,
  };
}

/** Every mark `purlis_core::panel::Mark` has, and a word it does not (drawn as the plain
 *  circle), each on a row of its own. */
const MARKS = ["todo", "persona", "repo", "piece", "note", "trouble", "vault", "dot", "unknown"];

/** Every tone `purlis_core::panel::Tone` has: plain, default and trouble. */
const TONES = ["plain", "default", "trouble"];

/**
 * A panel answer that draws every block the vocabulary has, every way it can be drawn: a note in
 * each of the three tones, facts, a list whose rows carry every mark and every tone — one of
 * them a memory, so charter's own views wrap it in its menu — and a chart in each of its two
 * shapes. `actions` is what each row offers, for an extension's view.
 */
function answered(actions: RowAction[] = []): ViewAnswer {
  return {
    kind: "answered",
    blocks: [
      ...TONES.map((tone) => ({ kind: "note" as const, text: `A ${tone} note`, tone })),
      { kind: "facts", facts: [{ label: "Vault", value: "none" }] },
      {
        kind: "list",
        rows: [
          row("memory", {
            text: "Charter defects go upstream",
            mark: "note",
            runs: "memory.open:shared/grill",
            actions,
          }),
          ...MARKS.map((mark) => row(`mark-${mark}`, { mark, actions })),
          ...TONES.map((tone) => row(`tone-${tone}`, { tone, actions })),
        ],
        empty: { headline: "Nothing remembered yet", body: null, offer: null },
      },
      {
        kind: "list",
        rows: [],
        empty: { headline: "Nothing here yet", body: "Add one.", offer: null },
      },
      ...(["bars", "columns"] as const).map((shape) => ({
        kind: "chart" as const,
        title: `Memories per persona, as ${shape}`,
        shape,
        unit: "memories",
        points: [
          { label: "steward", value: 3, note: "75%" },
          { label: "devops", value: 1, note: null },
          { label: "nobody", value: 0, note: null },
        ],
      })),
    ],
    took_ms: 2,
    overreach: null,
  };
}

/** What an extension's program answers: every block, rows that offer an action of each kind,
 *  and a sentence about what it changed outside its declared paths — drawn as trouble. */
const EXTENSION_ANSWER: ViewAnswer = {
  ...answered([
    { id: "pin", title: "Pin", asks_first: false, deletes: false },
    { id: "drop", title: "Drop…", asks_first: true, deletes: true },
  ]),
  overreach: "persona-statistics changed memory/x.md, which it does not declare it writes.",
} as ViewAnswer;

/** A workspace's changes (#474): one change's members are rows, so its list is headed by a
 *  Push and each member row offers Land. */
const CHANGES_ANSWER: ViewAnswer = {
  kind: "answered",
  blocks: [
    { kind: "note", text: "1 change in web", tone: "plain" },
    {
      kind: "list",
      rows: [
        row("site#12", { text: "site: tokens for every view", mark: "piece" }),
        row("api#3", { text: "api: the same", mark: "piece" }),
      ],
      empty: { headline: "No changes", body: null, offer: null },
    },
  ],
  took_ms: 40,
  overreach: null,
  changes: [
    {
      at: 1,
      change: "tokens",
      members: [
        { key: "site#12", repo: "site" },
        { key: "api#3", repo: "api" },
      ],
    },
  ],
};

/** A harness's card (HP-19), as `purlis_core::harness_card::Card::blocks` answers it. */
const HARNESS_CARD_ANSWER: ViewAnswer = {
  kind: "answered",
  blocks: [
    { kind: "note", text: "What opencode can do here", tone: "plain" },
    {
      kind: "facts",
      facts: [
        { label: "Program", value: "opencode" },
        { label: "Declared", value: "shipped with charter" },
      ],
    },
    {
      kind: "list",
      rows: [
        row("reports_waiting", {
          text: "Tells purlis when it is waiting for you",
          note: "yes",
          detail: null,
        }),
        row("ready_to_type", {
          text: "Can have a prompt typed in for you when it starts",
          note: "no",
          mark: "note",
          detail: {
            kind: "text",
            text: "opencode cannot have a prompt typed in for you, because purlis cannot tell when it has finished starting.",
          },
        }),
      ],
      empty: { headline: "Nothing to say", body: null, offer: null },
    },
    { kind: "note", text: "opencode says nothing until your first prompt.", tone: "plain" },
  ],
  took_ms: 1,
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

/** A project that picked a theme and a workspace colour: the theme group draws what is in
 *  force, and the colour's sentence. */
const THEME_PICKED: ProjectTheme = {
  ...THEME,
  picked: "charter-light",
  file: "charter.toml",
  draws: "charter-light",
  why: "charter.toml picks it",
  colour: "slate",
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

/** A vault that cannot be read, read through an identity still in purlis's environment. */
const VAULT_TROUBLED: VaultContents = {
  ...VAULT,
  health: { ok: false, detail: "the keyring is locked" },
  identity: [
    { variable: "OP_SERVICE_ACCOUNT_TOKEN", held: "environment" },
    { variable: "OP_CONNECT_TOKEN", held: "keyring" },
    { variable: "VAULT_TOKEN", held: "unset" },
  ],
  identity_in_app_env: ["OP_SERVICE_ACCOUNT_TOKEN"],
};

const PLANE_SAVING: PlaneSaving = {
  stage: "changed",
  changed: ["workspaces/web/workspace.md"],
  ahead: 0,
  pr: null,
  request: "pull request",
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
    request: "pull request",
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
  persona_hosts: [],
  resume_holds: false,
  persona_hosts_locked: null,
  dispatches: [{ persona: "devops", task: "check prod", outcome: "done" }],
};

/** One dispatch that ran and reported, and one still running (#1452). */
const DISPATCHES: DispatchRow[] = [
  {
    id: "01K6B",
    mode: "handoff",
    persona: "devops",
    task: "check prod",
    asker: "steward 3",
    asker_key: "01K6STEWARD",
    asker_persona: "steward",
    by_person: false,
    place: "web",
    folder: "workspaces/web",
    outcome: "running",
    started: "2026-10-07T12:00:00+00:00",
    ended: null,
    duration: "2m 5s",
    needed_you: 1,
    messages: 0,
    cost: null,
    tokens: null,
    brief: "Is the rollout healthy?",
    report: null,
    changed: null,
    open_session: 7,
    session_record: null,
    worktree: null,
  },
  {
    id: "01K6A",
    mode: "handoff",
    persona: null,
    task: "tidy the notes",
    asker: "steward 3",
    asker_key: "01K6STEWARD",
    asker_persona: "steward",
    by_person: false,
    place: "project root",
    folder: ".",
    outcome: "done",
    started: "2026-10-07T11:00:00+00:00",
    ended: "2026-10-07T11:00:45+00:00",
    duration: "45s",
    needed_you: 0,
    messages: 0,
    cost: "$0.42",
    tokens: "15k in, 4k out",
    brief: "Tidy the notes.",
    report: "Tidied.",
    changed: null,
    open_session: null,
    session_record: "sessions/20261007-110100-tidy.md",
    worktree: null,
  },
];

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

/** A workspace whose colour is a custom `#rrggbb`: the colour well is drawn, holding it. */
const WORKSPACE_COLOURED: WorkspaceSettings = {
  ...WORKSPACE_FILE,
  text: '{ "settings": { "theme": { "colour": "#2a9d8f" } } }',
  fields: [
    { path: [{ key: "theme" }, { key: "colour" }], value: { kind: "text", value: "#2a9d8f" } },
  ],
};

const INSTRUCTIONS: InstructionFile[] = [
  {
    repo: "site",
    file: "AGENTS.md",
    text: "# Agents\n\nRun the tests.\n",
    standing: { kind: "offered", caution: null },
  },
];

/** A machine with no harness, and Ollama answering on loopback (FR-29). */
const NO_HARNESS = {
  harnesses: [
    {
      name: "claude",
      title: "Claude Code",
      installed: false,
      signed_in: false,
      installer: "curl -fsSL https://claude.ai/install.sh | bash",
      installer_page: "https://code.claude.com/docs/en/setup",
    },
    {
      name: "codex",
      title: "Codex",
      installed: false,
      signed_in: false,
      installer: "curl -fsSL https://chatgpt.com/codex/install.sh | sh",
      installer_page: "https://github.com/openai/codex",
    },
  ],
  local_models: [{ title: "Ollama", base_url: "http://127.0.0.1:11434/v1", harness: "opencode" }],
};

/** What the core answers, by command; a function to answer from the arguments, an `Error` to
 *  refuse with its message. */
/** A chat's Activity (#1495): a task that asked, was answered and reported, with a file another
 *  task's report names too, a line long enough to be clipped, and a chat that has closed. */
const ACTIVITY: Activity = {
  name: "steward 3",
  key: "01K6STEWARD",
  undrawn: 1,
  lines: (
    [
      ["01K6D1", 0, "dispatched", "steward 3", "check prod", "Is the rollout healthy?"],
      ["01K6D2", 0, "dispatched", "steward 3", "lint", "Lint it. ".repeat(80)],
      ["01K6D1", 1, "question", "check prod", "steward 3", "Which host?"],
      ["01K6D1", 2, "answer", "steward 3", "check prod", "prod-2."],
      ["01K6D1", 3, "report", "check prod", "steward 3", "Scaled it up."],
      ["01K6D2", 1, "not listed", "lint", "steward 3", ""],
      ["01K6D2", 2, "stopped", "lint", "steward 3", "stopped by the operator"],
    ] as const
  ).map(([dispatch, n, kind, from, to, text], at) => ({
    dispatch,
    n,
    at: `2026-10-08T09:0${at}:00+00:00`,
    kind,
    from,
    from_key: from === "steward 3" ? "01K6STEWARD" : `01K6${from}`,
    from_session: from === "lint" ? null : 3,
    to,
    to_key: to === "steward 3" ? "01K6STEWARD" : `01K6${to}`,
    text: kind === "answer" || kind === "not listed" ? "" : text,
    by_person: n === 0 && dispatch === "01K6D1",
    asks: null,
    answers: null,
    unread: false,
    by_purlis: kind === "stopped" || kind === "not listed",
    // The answer's words were kept for 30 days and are gone.
    expired: kind === "answer",
    unkept: kind === "not listed" ? 2 : null,
    unkept_why: kind === "not listed" ? "count" : null,
    place: "alpha\u0000workspaces/alpha/svc",
    depth: 1,
    outcome: kind === "report" ? "done" : kind === "stopped" ? "stopped" : null,
    files: kind === "report" || kind === "stopped" ? ["deploy/values.yaml"] : [],
    task: dispatch === "01K6D1" ? "check prod" : "lint",
  })),
};

type Answers = Record<string, unknown>;

/** The core answering every question a view asks, as an ordinary plane would. */
const ORDINARY: Answers = {
  open_view: answered(),
  vault_open: VAULT,
  vault_secret_reveal: "postgres://db.internal/app",
  project_settings: SETTINGS,
  project_extensions: { extensions: [], local_left_out: null },
  project_harness_plugins: HARNESSES,
  project_saving_in_force: SAVING_IN_FORCE,
  project_theme: THEME,
  project_theme_drawn: null,
  plane_saving: PLANE_SAVING,
  workspace_saving: REPO_SAVING,
  session_record: RECORD,
  dispatches: { rows: DISPATCHES, undrawn: 1 },
  activity: ACTIVITY,
  memory_read: MEMORY,
  todo_read: {
    workspace: "alpha",
    slug: "20260302-091400-review",
    title: "Review the rollout plan",
    stamp: "2026-03-02",
    body: "Every region, **in full**.",
  },
  memory_archived: [
    {
      archived: "freeze",
      title: "Freeze",
      stamp: "2026-09-28 16:05",
      body: "No deploys on **Friday**.",
      path: "personas/_shared/memory/archive/freeze.md",
    },
  ],
  workspace_settings: WORKSPACE_FILE,
  repo_instructions: INSTRUCTIONS,
  start_options: {
    profiles: [
      {
        name: "claude",
        kind: "claude",
        shown: "claude",
        source: "built-in",
        is_default: true,
        approval: null,
        ready_to_type: true,
      },
      {
        name: "codex",
        kind: "codex",
        shown: "codex",
        source: "built-in",
        is_default: false,
        approval: "new",
        ready_to_type: true,
      },
    ],
    refused: [],
    personas: ["steward"],
    persona: "steward",
    persona_profiles: {},
    ignore_fix: null,
    declares_none: true,
  },
  branch_tree: {
    entries: [
      { name: "src", kind: "folder", ignored: false, refused: null },
      { name: "main.rs", kind: "file", ignored: false, refused: null },
    ],
    more: 0,
  },
  piece_file: { kind: "text", text: "fn main() {}\n" },
  harness_setup_found: NO_HARNESS,
  extensions_on: [],
};

function core(over: Answers = {}) {
  const answers = { ...ORDINARY, ...over };
  mockIPC((cmd) => {
    const answer = answers[cmd];
    if (answer instanceof Error) throw answer.message;
    return answer ?? null;
  });
}

/** Every heading row a view offers, as a catalogue row that runs — and one that cannot, so the
 *  disabled button is drawn too. */
function offerFor(id: string): Offer {
  const available = !id.endsWith(":ops");
  return {
    id,
    title: id,
    available,
    reason: available ? "" : "a vault in use cannot be removed",
    does: { verb: "chat.new" },
  };
}

/** One state a view tab draws: the view, what the core answers, what the operator does once it
 *  is drawn, and words that are on it once that state is reached. */
type State = {
  name: string;
  view: ViewRef;
  drawn: RegExp;
  answers?: Answers;
  waits?: boolean;
  before?: () => void;
  then?: () => Promise<void>;
};

const PERSONA: ViewRef = { from: null, view: "persona", key: "steward" };
const EXTENSION: ViewRef = { from: "persona-statistics", view: "statistics", key: "" };
const CHANGES: ViewRef = { from: null, view: "changes", key: WORKSPACE };
const MEMORY_REF: ViewRef = { from: null, view: "memory", key: "shared/grill" };
const ARCHIVE_REF: ViewRef = { from: null, view: "memory-archive", key: "shared" };
const VAULT_REF: ViewRef = { from: null, view: "vault", key: "ops" };
const TODO_REF: ViewRef = { from: null, view: "todo", key: "alpha/20260302-091400-review" };

const STATES: State[] = [
  { name: "a persona", view: PERSONA, drawn: /Charter defects go upstream/ },
  {
    name: "a persona that has gone",
    view: PERSONA,
    answers: { open_view: { kind: "gone", why: "steward was removed." } },
    drawn: /steward was removed/,
  },
  {
    name: "a persona that could not be read",
    view: PERSONA,
    answers: { open_view: new Error("persona.md could not be read: permission denied") },
    drawn: /permission denied/,
  },
  {
    name: "a vault, a secret revealed",
    view: VAULT_REF,
    drawn: /postgres:\/\/db\.internal/,
    then: async () => {
      await userEvent.click(await screen.findByRole("button", { name: "Reveal DB_URL" }));
    },
  },
  {
    name: "a vault that cannot be read",
    view: VAULT_REF,
    answers: { vault_open: VAULT_TROUBLED },
    drawn: /the keyring is locked/,
  },
  {
    name: "Settings, at the Project level",
    view: { from: null, view: "settings", key: "project" },
    drawn: /charter\.toml/,
  },
  {
    name: "Settings, at the Project level, its Appearance with a theme picked",
    view: { from: null, view: "settings", key: "project" },
    answers: { project_theme: THEME_PICKED, project_theme_drawn: "charter-light" },
    drawn: /charter\.toml picks it/,
    then: async () => {
      await userEvent.click(await screen.findByRole("button", { name: "Appearance" }));
    },
  },
  {
    name: "Settings, at the Project level, charter.toml as TOML",
    view: { from: null, view: "settings", key: "project" },
    drawn: /The whole file, comments and all/,
    then: async () => {
      await userEvent.click(
        within(await screen.findByRole("group", { name: "Edit as TOML" })).getByRole("button", {
          name: "charter.toml",
        }),
      );
    },
  },
  { name: "Saving", view: { from: null, view: "saving", key: "" }, drawn: /site/ },
  {
    name: "a workspace's changes, a landing refused",
    view: CHANGES,
    answers: {
      open_view: CHANGES_ANSWER,
      change_land_question: new Error("site#12 has a failing check"),
    },
    drawn: /site#12 has a failing check/,
    then: async () => {
      expect(await screen.findByRole("button", { name: /Push tokens/ })).toBeInTheDocument();
      await userEvent.click((await screen.findAllByRole("button", { name: "Land…" }))[0]);
    },
  },
  {
    name: "the project's dispatches, one read",
    view: { from: null, view: "dispatches", key: "" },
    drawn: /Tidied\./,
    then: async () => {
      await userEvent.click(
        await screen.findByRole("button", {
          name: "Show the brief and report of tidy the notes",
        }),
      );
    },
  },
  {
    name: "a chat's activity, a long line opened",
    view: { from: null, view: "activity", key: "3" },
    drawn: /lint's report also names deploy\/values\.yaml/,
    then: async () => {
      await userEvent.click(await screen.findByRole("button", { name: "Show all" }));
    },
  },
  {
    // #1496: a question its task is paused on, with the form that answers it open, and an
    // answer the person gave to an earlier one.
    name: "a chat's activity, a question being answered",
    view: { from: null, view: "activity", key: "3" },
    answers: {
      activity: {
        ...ACTIVITY,
        undrawn: 0,
        lines: [
          ACTIVITY.lines[0],
          { ...ACTIVITY.lines[2], n: 1 },
          { ...ACTIVITY.lines[3], n: 2, text: "prod-2.", expired: false, by_person: true },
          { ...ACTIVITY.lines[2], n: 3, text: "Which region?", asks: 6 },
        ],
      },
      activity_chat: 3,
    },
    drawn: /Enter sends, Shift\+Enter starts a new line/,
    then: async () => {
      await userEvent.click(
        await screen.findByRole("button", { name: "Answer check prod's question" }),
      );
    },
  },
  {
    name: "a chat's activity, none yet",
    view: { from: null, view: "activity", key: "3" },
    answers: { activity: { ...ACTIVITY, lines: [], undrawn: 0 } },
    drawn: /No activity yet/,
  },
  {
    name: "a chat's activity that could not be read",
    view: { from: null, view: "activity", key: "3" },
    answers: { activity: new Error("this project has been closed") },
    drawn: /could not read this chat's activity/,
  },
  {
    name: "the activity of a chat that is not open",
    view: { from: null, view: "activity", key: "3" },
    answers: { activity: null },
    drawn: /This chat is not open/,
  },
  {
    name: "the project's dispatches, none yet",
    view: { from: null, view: "dispatches", key: "" },
    answers: { dispatches: { rows: [], undrawn: 0 } },
    drawn: /No dispatches yet/,
  },
  {
    name: "a session record",
    view: { from: null, view: "session", key: RECORD.row.path },
    drawn: /The guard ran/,
  },
  { name: "a todo", view: TODO_REF, drawn: /in full/ },
  {
    name: "a todo closed since its tab opened",
    view: TODO_REF,
    answers: { todo_read: null },
    drawn: /not open any more/,
  },
  {
    name: "a todo purlis could not read",
    view: TODO_REF,
    answers: { todo_read: new Error("todos/ is a link out of the plane") },
    drawn: /link out of the plane/,
  },
  {
    name: "a harness's card",
    view: { from: null, view: "harness", key: "opencode" },
    answers: { open_view: HARNESS_CARD_ANSWER },
    drawn: /Can have a prompt typed in for you when it starts/,
  },
  {
    name: "a harness's card for a harness that has gone",
    view: { from: null, view: "harness", key: "gemini" },
    answers: {
      open_view: { kind: "gone", why: "This project has no harness called gemini any more." },
    },
    drawn: /no harness called gemini/,
  },
  { name: "a memory", view: MEMORY_REF, drawn: /offer menus/ },
  {
    name: "a memory being edited",
    view: MEMORY_REF,
    before: () => wantEdit(PLANE, MEMORY_REF.key),
    drawn: /^15 \/ 72$/,
  },
  {
    name: "a new memory",
    view: { from: null, view: "memory", key: `shared/${DRAFT}` },
    drawn: /0 \/ 72/,
  },
  {
    name: "Shared memory",
    view: { from: null, view: "shared-memory", key: "" },
    drawn: /Charter defects go upstream/,
  },
  {
    name: "a store's archive, one memory read",
    view: ARCHIVE_REF,
    drawn: /Restore memory/,
    then: async () => {
      await userEvent.click(await screen.findByRole("button", { name: /^Freeze/ }));
    },
  },
  {
    name: "a store's archive with nothing in it",
    view: ARCHIVE_REF,
    answers: { memory_archived: [] },
    drawn: /Nothing archived/,
  },
  {
    name: "a store's archive that cannot be read",
    view: ARCHIVE_REF,
    answers: { memory_archived: new Error("permission denied") },
    drawn: /could not read the shared archive/,
  },
  {
    name: "Settings at a workspace's level",
    view: { from: null, view: "workspace-settings", key: WORKSPACE },
    drawn: /Published with the project/,
  },
  {
    name: "Settings at a workspace's level, its custom colour",
    view: { from: null, view: "workspace-settings", key: WORKSPACE },
    answers: { workspace_settings: WORKSPACE_COLOURED },
    then: async () => {
      await userEvent.click(await screen.findByRole("button", { name: "Appearance" }));
    },
    drawn: /Custom colour/,
  },
  {
    name: "a workspace's repo instructions",
    view: { from: null, view: "repo-instructions", key: WORKSPACE },
    drawn: /AGENTS\.md/,
  },
  {
    name: "the first task, a command to approve",
    view: { from: null, view: "first-task", key: `/plane/workspaces/${WORKSPACE}/${WORKSPACE}` },
    drawn: /Second chat/,
  },
  {
    name: "a piece's files",
    view: { from: null, view: "piece-files", key: `${WORKSPACE}/${WORKSPACE}/chat-1` },
    drawn: /^main\.rs$/,
  },
  {
    name: "a file of a piece",
    view: { from: null, view: "piece-file", key: `${WORKSPACE}/${WORKSPACE}/chat-1/src/main.rs` },
    drawn: /fn main/,
  },
  {
    name: "the harness setup, with no harness",
    view: { from: null, view: "harness-setup", key: "/home/dev/web" },
    drawn: /^No harness found$/,
  },
  {
    name: "the harness setup, once one is installed",
    view: { from: null, view: "harness-setup", key: "/home/dev/web" },
    answers: {
      harness_setup_found: {
        harnesses: [{ ...NO_HARNESS.harnesses[0], installed: true }],
        local_models: [],
      },
    },
    drawn: /^A harness is installed$/,
  },
  {
    name: "Settings, at the You level",
    view: { from: null, view: "settings", key: "you" },
    drawn: /^Text$/,
  },
  {
    name: "a branch's files",
    view: { from: null, view: "piece-files", key: "alpha/svc/fix-it" },
    answers: {
      branch_tree: {
        entries: [{ name: "lib.rs", kind: "file", ignored: false, refused: null }],
        more: 0,
      },
      piece_file: { kind: "text", text: "x\n" },
    },
    drawn: /^lib\.rs$/,
  },
  {
    name: "one of a branch's files",
    view: { from: null, view: "piece-file", key: "alpha/svc/fix-it/README.md" },
    answers: { piece_file: { kind: "text", text: "Read me first\n" } },
    drawn: /Read me first/,
  },
  {
    name: "what changed in one of a branch's files",
    view: { from: null, view: "piece-diff", key: "alpha/svc/fix-it/notes.txt" },
    answers: {
      what_changed: {
        mark: "changed",
        from: null,
        uncommitted: true,
        base: "main",
        diff: {
          kind: "text",
          base: "kept line\n",
          head: "kept line\nadded line\n",
          hunks: [{ oldStart: 1, oldLines: 0, newStart: 2, newLines: 1 }],
        },
      },
    },
    drawn: /^added line$/,
  },
  {
    name: "a Search tab before anything is typed",
    view: { from: null, view: "search", key: "branch|alpha|alpha/svc/fix-it||" },
    drawn: /^Search the content of the files$/,
  },
  {
    name: "a Search tab whose query the core refused",
    view: { from: null, view: "search", key: "project|||r|open(" },
    answers: { search_files: new Error("regex parse error: unclosed group") },
    drawn: /unclosed group/,
  },
  {
    name: "an extension's view, its action refused",
    view: EXTENSION,
    answers: {
      open_view: EXTENSION_ANSWER,
      run_action: new Error("persona-statistics refused: busy"),
    },
    drawn: /refused: busy/,
    then: async () => {
      expect(await screen.findByText(/does not declare it writes/)).toBeInTheDocument();
      await userEvent.click((await screen.findAllByRole("button", { name: "Pin" }))[0]);
    },
  },
  {
    name: "an extension's view waiting to be asked",
    view: EXTENSION,
    waits: true,
    drawn: /was open when purlis last quit/,
  },
  {
    name: "an extension's view that could not answer",
    view: EXTENSION,
    answers: { open_view: new Error("persona-statistics timed out") },
    drawn: /timed out/,
  },
];

describe("every kind of view is drawn here", () => {
  it("has a state for every view charter has, and for an extension's", () => {
    // A new view gets a glyph in `OWN_MARKS` the day it is added, so a view this file has not
    // heard of fails here rather than going unchecked.
    const drawnHere = new Set(
      STATES.filter((one) => one.view.from === null).map((one) => one.view.view),
    );
    expect(Object.keys(OWN_MARKS).filter((view) => !drawnHere.has(view))).toEqual([]);
    expect(STATES.some((one) => one.view.from !== null)).toBe(true);
  });
});

describe.each(Object.keys(BUILT_IN))("every view in %s", (theme) => {
  beforeEach(() => drawIn(BUILT_IN[theme]));
  afterEach(() => {
    drawIn(DEFAULT_THEME);
    forgetDrafts();
    // Which group each Settings level shows is the window's, for as long as it runs (SE-22).
    forgetGroups();
  });

  it.each(STATES)("draws $name with tokens only", async (state) => {
    core(state.answers);
    state.before?.();
    const { container } = render(
      <ViewPane
        plane={PLANE}
        view={state.view}
        title="The view"
        workspace={WORKSPACE}
        waits={state.waits ?? false}
        offered={[
          {
            extension: "persona-statistics",
            id: "statistics",
            title: "Statistics",
            about: "personas",
          },
        ]}
        onOpenView={() => {}}
        onAsk={() => {}}
        onVaultChanged={() => {}}
        offerFor={offerFor}
        onPress={() => {}}
      />,
    );
    await state.then?.();
    expect((await screen.findAllByText(state.drawn)).length).toBeGreaterThan(0);
    expect(complaints(container)).toEqual([]);
  });
});
