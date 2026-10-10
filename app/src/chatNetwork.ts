import type { ViewRef } from "./tabs";

/**
 * **A chat's Network view in the window** (#1662, spec #1661): what one chat can reach now and
 * what it was refused, read once from the core (`chat_network`) as the tab opens.
 *
 * It is a view tab, `{ from: null, view: "chat-network", key: "<session>" }`: one per chat,
 * opened from the chat tab's menu and from the palette (`tab.network:<tab>`), and keyed by the
 * chat's number as a chat's Activity is (`activity.ts`). A tab a launch brings back asks for
 * that number again, and says the chat is not open where the launch did not bring it back.
 */
export const CHAT_NETWORK_VIEW = "chat-network";

/** The Network tab of the chat in `session`. */
export function chatNetworkView(session: number): ViewRef {
  return { from: null, view: CHAT_NETWORK_VIEW, key: String(session) };
}

/** What the Network tab of the chat called `name` is called. */
export function chatNetworkTitle(name: string): string {
  return `Network · ${name}`;
}

/** Whether `view` is a chat's Network tab. */
export function isChatNetwork(view: ViewRef): boolean {
  return view.from === null && view.view === CHAT_NETWORK_VIEW;
}

/** The chat a Network tab is about; `undefined` for a key that is not a chat's number. */
export function chatNetworkSession(view: ViewRef): number | undefined {
  return /^[1-9]\d*$/.test(view.key) ? Number(view.key) : undefined;
}
