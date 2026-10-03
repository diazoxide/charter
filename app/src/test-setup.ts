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

// jsdom has no layout, so a Range has no client rects. CodeMirror measures one when it scrolls
// to a line (RC-20 opens a file at a line), and the measure runs after the test that caused it.
// An empty rect list is what an unlaid-out document would report.
if (typeof Range !== "undefined") {
  Range.prototype.getClientRects ??= () => [] as unknown as DOMRectList;
  Range.prototype.getBoundingClientRect ??= () => new DOMRect(0, 0, 0, 0);
}
