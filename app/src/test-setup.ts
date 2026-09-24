import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";
import { forgetExtensionThemes } from "./Extensions";
import { forgetExtensionsOn } from "./extensionsOn";
import { forgetProjectThemes } from "./projectTheme";

// What each project has on is kept per plane for the window's life (`extensionsOn.ts`); every
// test's window is a new one, and most of them share a plane path.
afterEach(() => {
  forgetExtensionsOn();
  forgetExtensionThemes();
  forgetProjectThemes();
});

// jsdom has no ResizeObserver, and the panes and their splits watch their own size with one.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
