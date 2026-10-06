import type { SandboxPolicy, SandboxPreset, SandboxState, SettingsFile } from "../bindings";
import { key, onOffAt, textAt, valueAt, type Control, type Shown } from "./fileControls";
import type { LiveSetting } from "./groups";

/**
 * **Settings › Project › Sandbox explains what a chat here can do** (#1340, spec #1330): one
 * sentence at the top ({@link whatAChatCanDo}), the mode as a status line, Internet access as
 * the core's presets — each opening to the hosts the core lists for it — the certificate check,
 * and what chats can change and what is always protected, each with why.
 *
 * **Every preset and host is the core's** (`sandbox_state`'s `presets`, from
 * `purlis_core::sandbox::Preset` and `hosts`): nothing here lists a host. Which presets are on is
 * read from the committed file as it stands, so the sentence follows a box as it is ticked.
 *
 * **An administrator's policy has the last word** (#1343, ADR 0067 §1): each value it locks is
 * shown as "Locked by policy", with who set it ({@link SandboxPolicy}'s `locked_by`), and offers
 * no control. The presets in force are the project's within the policy's.
 */

/** `[sandbox] mode`, which this page turns on and never back off (D-SE17g). */
export const SANDBOX_MODE = key("sandbox", "mode");

/** `[sandbox] egress`: the presets turned on. */
const EGRESS = key("sandbox", "egress");

/** `[sandbox] certificate-checks` (#1337): off unless the file says `true`. */
const CERTIFICATE_CHECKS = key("sandbox", "certificate-checks");

/** The preset whose caches a chat may write while it is on (#1337): the core's word for it. */
const PACKAGES = "toolchains";

/**
 * The presets in force as `shared` stands: those `egress` names, in the core's order; every
 * preset where the file names none, as the core reads a missing `egress` (ADR 0067 §3).
 */
export function presetsOn(
  shared: Shown,
  table: readonly SandboxPreset[],
  policy: SandboxPolicy | null = null,
): SandboxPreset[] {
  const value = valueAt(shared, EGRESS);
  const asked =
    value?.kind !== "list"
      ? [...table]
      : table.filter((preset) => value.value.includes(preset.word));
  // The strictest wins: a preset the policy does not allow is off, whatever the file says.
  return policy?.presets == null
    ? asked
    : asked.filter((preset) => policy.presets?.includes(preset.word));
}

/** What the sentence at the top is made from. */
export type Reach = {
  /** Whether the project turned the sandbox on. */
  on: boolean;
  /** The presets turned on, in the core's order. */
  presets: readonly SandboxPreset[];
  /** The project's own hosts every chat is granted, as the core counts them. */
  projectHosts: number;
  /** Your own hosts every chat here is granted on this machine, as the core counts them. */
  yourHosts: number;
  /** The folders you let every chat here write on this machine, as the core counts them. */
  folders: number;
  /** Whether a chat may ask the system's certificate service: certificate checks on, on a Mac,
   *  the only system where the setting widens anything (D-1337-7). */
  certificateService: boolean;
};

/** `a`, `a and b`, `a, b and c`. */
function listed(items: readonly string[]): string {
  if (items.length < 2) return items.join("");
  return `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`;
}

/** A preset's name inside a sentence: "Code hosting" as "code hosting", "AI providers" kept. */
function midSentence(title: string): string {
  return /^[A-Z][a-z]/.test(title) ? title[0].toLowerCase() + title.slice(1) : title;
}

/** `1 thing`, `2 things`. */
function counted(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** **The sentence at the top**: what a chat here can change and reach, as the files stand. */
export function whatAChatCanDo(reach: Reach): string {
  if (!reach.on)
    return "Chats here run without a sandbox, so a chat can change any file you can and reach any host.";
  const caches = reach.presets.some((preset) => preset.word === PACKAGES);
  const places = [
    "the folder it works in",
    ...(caches ? ["in the project's package caches"] : []),
    ...(reach.folders > 0
      ? [
          `in ${counted(reach.folders, "folder you allowed on this machine", "folders you allowed on this machine")}`,
        ]
      : []),
  ];
  const reached = [
    ...reach.presets.map((preset) => midSentence(preset.title)),
    ...(reach.projectHosts > 0
      ? [counted(reach.projectHosts, "project host", "project hosts")]
      : []),
    ...(reach.yourHosts > 0 ? [counted(reach.yourHosts, "host of yours", "hosts of yours")] : []),
    ...(reach.certificateService ? ["the system's certificate service"] : []),
  ];
  return `A chat here can change files in ${listed(places)}, and reach ${
    reached.length > 0 ? listed(reached) : "no host on the internet"
  }.`;
}

/** A sentence that starts with a capital: the core's opt-out count starts lowercase. */
function capitalised(said: string): string {
  return said.length > 0 ? said[0].toUpperCase() + said.slice(1) : said;
}

/**
 * **The mode as a status line** (#1340): a project's sandbox is on for everyone or not set, and
 * the only way to run one chat without it is that chat's own tab (ADR 0067 §7). Turn the sandbox
 * on writes `mode = "on"`; nothing here takes it back off, and it has no Undo (D-SE17g).
 */
function modeStatus(sandbox: SandboxState | undefined): Control {
  const policy = sandbox?.policy ?? null;
  return {
    ...textAt(SANDBOX_MODE, "Sandbox"),
    kind: "status",
    hint: "Settings can turn it on, never off.",
    // Any `mode` reads as on: one that is not "on" is refused and read as on (ADR 0067).
    status: (value) =>
      value !== ""
        ? [
            policy?.opt_out === true
              ? `On for everyone in this project, and no chat runs without it. ${policy.locked_by}`
              : "On for everyone in this project. To run one chat without it, use that chat's tab.",
            // The opt-out count is this machine's, and is never sent (ADR 0067 §7, V78 d).
            ...(sandbox?.on === true && sandbox.said !== null
              ? [`${capitalised(sandbox.said)}. Counted on this machine only, and never sent.`]
              : []),
          ].join(" ")
        : "Off in this project.",
    turnOn: { label: "Turn the sandbox on", value: "on" },
  };
}

/**
 * **Internet access** (#1340, `CONTEXT.md`): one box per preset, named as the core names it, each
 * opening to the hosts the core lists for it. Unticking every box writes `[]`, which reaches no
 * host, rather than taking the key out, which would reach every preset.
 */
function internetAccess(table: readonly SandboxPreset[], policy: SandboxPolicy | null): Control {
  const allowed = policy?.presets ?? null;
  const lockedBy = policy?.locked_by ?? "";
  if (allowed !== null)
    // Locked: what is on, said, and no box to tick (#1343).
    return {
      ...textAt(EGRESS, "Internet access"),
      kind: "status",
      hint: "The hosts every chat here may reach, besides the project's own below.",
      read: (file) =>
        presetsOn(file, table, policy)
          .map((preset) => preset.word)
          .join("\n"),
      status: (value) => {
        const on = table.filter((preset) => value.split("\n").includes(preset.word));
        return `${
          on.length > 0
            ? `${listed(on.map((preset) => preset.title))} ${on.length === 1 ? "is" : "are"} on.`
            : "No preset is on."
        } ${lockedBy}`;
      },
    };
  return {
    id: JSON.stringify(EGRESS),
    label: "Internet access",
    hint: "The hosts every chat here may reach, besides the project's own below. A host nothing allows is blocked, and the chat's tab says so.",
    kind: "checks",
    options: table.map((preset) => ({
      value: preset.word,
      label: preset.title,
      says: (
        <details className="sandbox-hosts">
          <summary>{counted(preset.hosts.length, "host", "hosts")}</summary>
          <ul>
            {preset.hosts.map((host) => (
              <li key={host}>
                <code>{host}</code>
              </li>
            ))}
          </ul>
        </details>
      ),
    })),
    read: (file) => {
      const value = valueAt(file, EGRESS);
      return value?.kind === "list"
        ? value.value.join("\n")
        : table.map((preset) => preset.word).join("\n");
    },
    edits: (draft) => [
      {
        path: EGRESS,
        value: {
          kind: "list",
          value: draft
            .split("\n")
            .map((one) => one.trim())
            .filter((one) => one !== ""),
        },
      },
    ],
  };
}

/** `[sandbox] certificate-checks` (#1337, D-1337-7), off by default. */
function certificateChecks(): Control {
  return onOffAt(
    CERTIFICATE_CHECKS,
    "Certificate checks",
    "Lets programs such as gh and other Go tools verify a host's certificate through the system on macOS. Off by default, because the system then fetches addresses a certificate names, outside the hosts above.",
    "not set — off",
  );
}

/** The sandbox's settings kept in the committed file, in the order the page draws them. */
export function sandboxControls(sandbox: SandboxState | undefined): Control[] {
  return [
    modeStatus(sandbox),
    internetAccess(sandbox?.presets ?? [], sandbox?.policy ?? null),
    certificateChecks(),
  ];
}

/** What the top says while the core has not said what the sandbox grants (#1340). */
export const READING = "Reading this project's sandbox…";

/**
 * The sentence at the top, as the committed file and the core say it now. Where the sandbox is
 * on and the core has not answered — or could not — it says it is reading, never a reach it
 * does not know (#1340).
 */
export function sandboxSentence(
  shared: SettingsFile,
  sandbox: SandboxState | undefined,
  mac: boolean,
): string {
  const on = valueAt(shared, SANDBOX_MODE) !== undefined || sandbox?.on === true;
  if (!on) return whatAChatCanDo({ ...NOTHING, on: false });
  if (sandbox === undefined) return READING;
  const cert = valueAt(shared, CERTIFICATE_CHECKS);
  return whatAChatCanDo({
    on,
    presets: presetsOn(shared, sandbox.presets, sandbox.policy),
    projectHosts: sandbox.besides.project_hosts,
    yourHosts: sandbox.besides.your_hosts,
    folders: sandbox.besides.folders,
    certificateService: mac && cert?.kind === "bool" && cert.value,
  });
}

/** A reach of nothing at all. */
const NOTHING: Reach = {
  on: true,
  presets: [],
  projectHosts: 0,
  yourHosts: 0,
  folders: 0,
  certificateService: false,
};

/**
 * The sentences under the top one: what a chat can read; each persona's own hosts, and who
 * else holds them; and the harnesses never sandboxed on this machine.
 */
export function sandboxNotes(shared: Shown, sandbox: SandboxState | undefined): string[] {
  if (valueAt(shared, SANDBOX_MODE) === undefined && sandbox?.on !== true) return [];
  const policy = sandbox?.policy ?? null;
  const lockedBy = policy?.locked_by ?? "";
  const personaLocked = policy?.persona_hosts === true;
  const allowedHosts = policy?.hosts ?? null;
  return [
    // Reads are confined only by the classes (ADR 0067 §5): your own logins are not hidden.
    "A chat can read any file you can, except vaults and purlis's own keys, so keep secrets in a vault.",
    ...(personaLocked
      ? (sandbox?.persona_hosts ?? []).map(
          (one) =>
            `A chat as ${one.persona} reaches none of its own hosts (${listed(one.hosts)}). ${lockedBy}`,
        )
      : (sandbox?.persona_hosts ?? []).map(
          (one) => `A chat as ${one.persona} also reaches ${listed(one.hosts)}.`,
        )),
    ...(allowedHosts !== null
      ? [
          allowedHosts.length > 0
            ? `Only these hosts may be added, by the project or its forges, you, a persona or a block's Allow: ${listed(allowedHosts)}. Any other is not reached. ${lockedBy}`
            : `No host may be added, by the project or its forges, you, a persona or a block's Allow. ${lockedBy}`,
        ]
      : []),
    ...(!personaLocked && (sandbox?.persona_hosts ?? []).length > 0
      ? [
          // start::grants_persona, D-1362-5 and D-1362-6.
          "A chat that names no persona reaches its default persona's hosts. A chat opened by a handoff may hold the asking chat's hosts, and a Resume the default persona's, until you allow its own on its tab.",
        ]
      : []),
    ...(sandbox !== undefined && sandbox.never.length > 0
      ? [`On this machine these run without it: ${sandbox.never.join("; ")}.`]
      : []),
  ];
}

/** One item of a read-only list: what, and why in one sentence. */
type Reason = { what: string; why: string };

/**
 * **What a sandboxed chat may change** (ADR 0067 §2, #1332, #1333, #1335, #1337, #1342): its
 * own folder and temp folder; the project's package caches with Package registries; what it asks
 * purlis to write for it; and what a person granted it.
 */
function canChange(caches: boolean): Reason[] {
  return [
    {
      what: "The folder it works in",
      why: "The project, workspace or clone the chat was opened in, where its work goes.",
    },
    { what: "Its temporary files", why: "Programs need somewhere to put scratch files." },
    {
      what: "The harness's own state",
      why: "What the harness keeps for the chat, such as its session data, so the chat can run.",
    },
    ...(caches
      ? [
          {
            what: "The project's package caches",
            why: "So builds can download packages. They are this project's own, and your caches are only read.",
          },
        ]
      : []),
    {
      what: "Session records, memory and todos",
      why: "The chat asks purlis to write them, so they work from any chat, one in a clone included.",
    },
    {
      what: "Clones and worktrees",
      why: "The chat asks purlis, which writes the git settings the sandbox keeps from the chat.",
    },
    {
      what: "Folders and hosts you allow",
      why: "From a block's Allow, each listed under Granted, where you can revoke it. A folder is only ever one inside the project, or one you list there, never a temp folder.",
    },
  ];
}

/** **What no chat may change, whatever is granted** (ADR 0067 §5): the denial classes. */
function alwaysProtected(shared: string, local: string): Reason[] {
  return [
    {
      what: "Vaults",
      why: "A chat cannot read where secrets are kept. A command gets a secret only through purlis.",
    },
    {
      what: "What purlis records about chats",
      why: "Only purlis writes it, so a chat cannot change the record of what it did.",
    },
    {
      what: "Your approvals in purlis",
      why: "Only you approve what runs and what is granted, so a chat cannot approve itself.",
    },
    {
      what: "Files other programs load later",
      why: "Git's config and hooks, shell startup files, and editor and harness settings. A program run outside the sandbox would run what a chat put there.",
    },
    {
      what: `This project's ${shared} and ${local}`,
      why: "At the project root and in each folder from the chat's up to it, because they set the sandbox itself.",
    },
  ];
}

/**
 * **A value an administrator's policy locks** (#1343), as a row of its own: what it says —
 * "Locked by policy" and who set it — and no control.
 */
export function lockedRow(id: string, label: string, said: string): LiveSetting {
  return {
    id,
    label,
    help: "",
    useControl: () => ({
      grouped: true,
      control: (ids) => (
        <p
          id={ids.id}
          className="ui-setting-status"
          aria-labelledby={ids.labelledBy}
          aria-describedby={ids.describedBy}
        >
          {said}
        </p>
      ),
    }),
  };
}

/** A read-only list of reasons, as a row of its own. */
function reasons(id: string, label: string, help: string, items: readonly Reason[]): LiveSetting {
  return {
    id,
    label,
    help,
    useControl: () => ({
      grouped: true,
      control: (ids) => (
        <ul
          id={ids.id}
          className="sandbox-reasons"
          aria-labelledby={ids.labelledBy}
          aria-describedby={ids.describedBy}
        >
          {items.map((one) => (
            <li key={one.what}>
              <span className="sandbox-what">{one.what}</span>
              <span className="sandbox-why">{one.why}</span>
            </li>
          ))}
        </ul>
      ),
    }),
  };
}

/** The two read-only lists, drawn under the project's hosts. */
export function sandboxReasons(
  shared: SettingsFile,
  local: SettingsFile,
  sandbox: SandboxState | undefined,
): LiveSetting[] {
  const when = valueAt(shared, SANDBOX_MODE) !== undefined ? "" : " Once the sandbox is on.";
  const caches = presetsOn(shared, sandbox?.presets ?? [], sandbox?.policy ?? null).some(
    (one) => one.word === PACKAGES,
  );
  return [
    reasons(
      "project.sandbox.changes",
      "What chats can change",
      `What a sandboxed chat may write.${when}`,
      canChange(caches),
    ),
    reasons(
      "project.sandbox.protected",
      "Always protected",
      `What no chat may change, whatever you allow.${when}`,
      alwaysProtected(shared.file, local.file),
    ),
  ];
}
