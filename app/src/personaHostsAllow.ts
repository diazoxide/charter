import { commands, type PersonaHosts, type PlaneId, type SandboxState } from "./bindings";

/**
 * **Whether this page can make a window-only call** (`WINDOW_ONLY` in the session protocol): the
 * window's own client has the method, and the UI RPC's client a link gets has none of them. So a
 * control that would press one is drawn only where the press can work (#1362).
 */
export function inTheWindow(client: object, method: string): boolean {
  return typeof (client as Record<string, unknown>)[method] === "function";
}

/** Whether this page may allow a persona's hosts: the window's alone. */
export const mayAllowPersonaHosts = (): boolean => inTheWindow(commands, "allowPersonaHosts");

/** "a", "a and b", "a, b and c". */
export function listedHosts(hosts: readonly string[]): string {
  if (hosts.length < 2) return hosts.join("");
  return `${hosts.slice(0, -1).join(", ")} and ${hosts[hosts.length - 1]}`;
}

/** What an Allow of `one` is for, as its button says it. */
export function allowLabel(one: PersonaHosts): string {
  return one.default
    ? `Allow for ${one.persona} chats and chats that name no persona`
    : `Allow for ${one.persona} chats`;
}

/** Who reaches `one`'s hosts once allowed, as a clause: its chats, and for the default persona
 *  every chat that names no persona too (D-1362-12). */
export function whoReaches(one: PersonaHosts): string {
  return one.default
    ? `chats as ${one.persona}, and every chat that names no persona (${one.persona} is this project's default persona),`
    : `chats as ${one.persona}`;
}

/** When an Allow reaches a chat: a sandbox is compiled as a chat starts. */
export const FROM_NEXT_START =
  "Chats started after you allow them reach them; a chat already running takes them when it restarts.";

/**
 * **Allows `one`'s hosts on this machine** as they were shown (`allow_persona_hosts`, the digest
 * of that list): the answer is the project's sandbox state now, or the core's sentence why not.
 */
export async function allowPersonaHosts(
  plane: PlaneId,
  one: PersonaHosts,
): Promise<{ state: SandboxState } | { refused: string }> {
  try {
    const answer = await commands.allowPersonaHosts(plane, one.persona, one.digest);
    return answer.status === "ok" ? { state: answer.data } : { refused: answer.error };
  } catch (err: unknown) {
    return { refused: String(err) };
  }
}
