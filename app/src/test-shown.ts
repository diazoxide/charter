/**
 * **The window out of sight, or back in it**, for the tests of what reads on a beat
 * (`whileShown.ts`): the state jsdom reports, then the event the engine sends.
 *
 * jsdom (with vitest's `pretendToBeVisual`) always says "visible"; this answers for it until
 * {@link forgetShown}, which every test that calls it runs after itself.
 */
export function windowShown(visible: boolean): void {
  Object.defineProperty(document, "visibilityState", {
    configurable: true,
    get: () => (visible ? "visible" : "hidden"),
  });
  Object.defineProperty(document, "hidden", { configurable: true, get: () => !visible });
  document.dispatchEvent(new Event("visibilitychange"));
}

/** Shows the window again and hands its state back to jsdom. */
export function forgetShown(): void {
  windowShown(true);
  Reflect.deleteProperty(document, "visibilityState");
  Reflect.deleteProperty(document, "hidden");
}
