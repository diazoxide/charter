import "@testing-library/jest-dom/vitest";

// jsdom has no ResizeObserver, and the panes and their splits watch their own size with one.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
