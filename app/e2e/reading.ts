import { browser } from "@wdio/globals";

/**
 * Reading a list the app is drawing — a strip's tabs, their names, their paths — in ONE pass
 * inside the page, so the read cannot fall between two renders.
 *
 * **Why not `$$(…).getElements()` and then a read of each** (charter#506): that is two trips,
 * and a list that changes between them leaves the second holding an element that is gone.
 * WebdriverIO does not fail such a read — it recovers it: it finds the element again by the
 * selector and the index it was found at, and when the list is now shorter it waits for that
 * index to come back. It never does. Measured on macOS and seen on Linux CI, one read of a tab
 * that went between the find and the read held the caller for 80 s, and then failed with the
 * stale element anyway. Inside a `waitUntil` of 30 s that is a timeout whose reason names a
 * stale element — while the strip had long since shown exactly what was being waited for.
 *
 * A read done by one `browser.execute` sees the document as it is at that moment and returns
 * plain values, which cannot go stale. A helper that polls a list which changes while it is
 * polled reads it through here.
 */

/** Each element matching `selector`: the attributes named, read together, in document order. */
export async function attributesOfEach<Name extends string>(
  selector: string,
  names: readonly Name[],
): Promise<Record<Name, string | null>[]> {
  return browser.execute(
    (within: string, wanted: readonly string[]) =>
      [...document.querySelectorAll(within)].map((one) =>
        Object.fromEntries(wanted.map((name) => [name, one.getAttribute(name)])),
      ),
    selector,
    names,
  ) as Promise<Record<Name, string | null>[]>;
}

/**
 * Each element matching `selector`: the text it draws, in document order.
 *
 * `innerText`, trimmed, because that is what WebdriverIO's `getText` answered for the same
 * element: the rendered text, not the markup's.
 */
export async function textOfEach(selector: string): Promise<string[]> {
  return browser.execute(
    (within: string) =>
      [...document.querySelectorAll(within)].map((one) => (one as HTMLElement).innerText.trim()),
    selector,
  );
}
