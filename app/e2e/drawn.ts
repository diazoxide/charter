import { browser } from "@wdio/globals";

/**
 * **Waits until the window is drawn, and so ready for the keyboard** (#1469).
 *
 * The Tauri service hands a spec its session as soon as the window EXISTS: it waits for a
 * window handle and for its own plugin, not for the app. On a cold start the page can still be
 * loading its bundle then — `window.__TAURI__` is there (the core injects it before any page
 * script), so a spec's first `invoke` answers, but nothing of the app is drawn yet and a key
 * sent now reaches an empty `<body>`. The first spec of a run is the only one that starts that
 * early, and on a slow Linux runner its first `F2` was lost: the palette never came and every
 * case after it fell with it.
 *
 * The condition is the app's own root, `<main class="window">`, in the document. The palette's
 * key listener is added in the same commit that puts it there (`Palette.tsx`, a layout effect),
 * so once it is drawn a key is heard. Read inside the page, one `execute` per poll; a poll the
 * page cannot answer yet counts as not drawn.
 */
export async function theWindowIsDrawn(): Promise<void> {
  await browser.waitUntil(
    async () => {
      try {
        return await browser.execute(() => document.querySelector("main.window") !== null);
      } catch {
        // A page still between documents answers nothing yet; the next poll asks again.
        return false;
      }
    },
    {
      timeout: 60_000,
      interval: 250,
      timeoutMsg: 'the app never drew its window: no <main class="window"> in 60 seconds',
    },
  );
}
