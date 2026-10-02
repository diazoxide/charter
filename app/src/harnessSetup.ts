/**
 * What the harness setup tab (FR-29) asks of the project that holds it: a shell tab with a
 * harness's official installer typed in, or the picker in the directory the tab is about. A
 * window event, as the Saving tab's ways out are (`saving.ts`): the tab is drawn deep inside
 * the project's layout, and the project is what opens tabs.
 *
 * **It names a harness and never carries a command** (ruling V65). The installer the shell
 * runs is the core's own compiled-in line (`type_installer`), so nothing that can dispatch a
 * window event can make a shell run words of its choosing through here.
 */
export const HARNESS_SETUP = "charter-harness-setup";

/** What the tab asks for, of which project: `harness` is a harness's name (`claude`, `codex`,
 *  `opencode`) — the one to install, or the one the picker starts on. `cwd` is where the chat
 *  starts; `workspace` is the strip the installer's shell is filed on, beside the tab. */
export type HarnessSetupAsk = {
  plane: string;
  harness: string;
} & ({ way: "install"; workspace: string } | { way: "chat"; cwd: string });

/** Ask the project `ask.plane` to open what `ask` names. */
export function askHarnessSetup(ask: HarnessSetupAsk): void {
  window.dispatchEvent(new CustomEvent<HarnessSetupAsk>(HARNESS_SETUP, { detail: ask }));
}
