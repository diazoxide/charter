import { describe, expect, it } from "vitest";
import {
  contentsOf,
  keepView,
  noTabs,
  openPreview,
  openTab,
  openView,
  previewOf,
  showInstead,
  type Tabs,
  type ViewRef,
} from "./tabs";

/**
 * **The preview tab** (SI-9b, ADR 0065 Q1), VS Code's model: a single click on a memory row
 * shows it in the strip's one preview tab, replacing whatever that tab previewed; a double-click
 * or starting an edit keeps it, and a kept tab is an ordinary tab that nothing replaces.
 */
const memory = (slug: string): ViewRef => ({
  from: null,
  view: "memory",
  key: `persona/steward/${slug}`,
});

function leadOf(tabs: Tabs, id: number) {
  const content = contentsOf(tabs, id)[0]?.content;
  if (content?.kind !== "view") throw new Error("not a view tab");
  return content;
}

describe("a preview tab", () => {
  it("opens in front, marked as a preview", () => {
    const tabs = openPreview(openTab(noTabs(), 11), memory("a"), "A", "alpha");

    expect(tabs.order).toHaveLength(2);
    expect(tabs.inFront).toBe(tabs.order[1]);
    expect(leadOf(tabs, tabs.order[1]).preview).toBe(true);
    expect(previewOf(tabs, "alpha")).toBe(tabs.order[1]);
  });

  it("is replaced in place by the next single click, and no second tab opens", () => {
    const first = openPreview(openTab(noTabs(), 11), memory("a"), "A", "alpha");
    const id = first.order[1];

    const second = openPreview(first, memory("b"), "B", "alpha");

    expect(second.order).toEqual(first.order);
    expect(second.inFront).toBe(id);
    expect(second.byId[id].name).toBe("B");
    expect(leadOf(second, id).view).toEqual(memory("b"));
    expect(leadOf(second, id).preview).toBe(true);
  });

  it("brings forward a memory already open rather than previewing it twice", () => {
    const kept = openView(noTabs(), memory("a"), "A", "alpha");
    const previewing = openPreview(kept, memory("b"), "B", "alpha");

    const again = openPreview(previewing, memory("a"), "A", "alpha");

    expect(again.order).toEqual(previewing.order);
    expect(again.inFront).toBe(kept.order[0]);
    // …and the preview it passed over is left as it was.
    expect(leadOf(again, previewing.order[1]).view).toEqual(memory("b"));
  });

  it("once kept, is never replaced: the next click previews in a tab of its own", () => {
    const opened = openPreview(noTabs(), memory("a"), "A", "alpha");
    const kept = keepView(opened, memory("a"));

    expect(leadOf(kept, kept.order[0]).preview).toBeFalsy();
    expect(previewOf(kept, "alpha")).toBeUndefined();

    const next = openPreview(kept, memory("b"), "B", "alpha");

    expect(next.order).toHaveLength(2);
    expect(leadOf(next, next.order[0]).view).toEqual(memory("a"));
    expect(leadOf(next, next.order[1]).view).toEqual(memory("b"));
  });

  it("is one per strip: a preview on another workspace's strip is not replaced", () => {
    const beta = openPreview(noTabs(), memory("a"), "A", "beta");

    const alpha = openPreview(beta, memory("b"), "B", "alpha");

    expect(alpha.order).toHaveLength(2);
    expect(leadOf(alpha, alpha.order[0]).view).toEqual(memory("a"));
  });

  it("is not a tab split beside a chat: a view pane with a chat beside it is left alone", () => {
    // Only a tab that shows the preview and nothing else is the preview tab: replacing one
    // with a chat in it would be replacing the chat's tab.
    const opened = openPreview(noTabs(), memory("a"), "A", "alpha");
    const split: Tabs = {
      ...opened,
      byId: {
        ...opened.byId,
        [opened.order[0]]: {
          ...opened.byId[opened.order[0]],
          layout: {
            kind: "split",
            direction: "row",
            children: [
              opened.byId[opened.order[0]].layout,
              { kind: "pane", pane: 99, content: { kind: "session", session: 11, chat: "1" } },
            ],
          },
        },
      },
    };

    expect(previewOf(split, "alpha")).toBeUndefined();
  });

  it("an ordinary openView is never a preview", () => {
    const tabs = openView(noTabs(), memory("a"), "A", "alpha");

    expect(leadOf(tabs, tabs.order[0]).preview).toBeFalsy();
    expect(previewOf(tabs, "alpha")).toBeUndefined();
  });
});

describe("a view shown instead of another", () => {
  it("keeps the tab, its place and its keep, and takes the new name", () => {
    // A new memory's tab, once it is saved, shows the memory it made: same tab, now named for
    // it, and keyed by its slug so a click on its row finds it.
    const draft = memory("+");
    const tabs = openView(openTab(noTabs(), 11), draft, "New memory", "alpha");
    const id = tabs.order[1];

    const saved = showInstead(tabs, draft, memory("where-prod-1-is"), "Where prod-1 is");

    expect(saved.order).toEqual(tabs.order);
    expect(saved.byId[id].name).toBe("Where prod-1 is");
    expect(leadOf(saved, id).view).toEqual(memory("where-prod-1-is"));
    expect(leadOf(saved, id).preview).toBeFalsy();
  });

  it("changes nothing when the view is not shown", () => {
    const tabs = openTab(noTabs(), 11);

    expect(showInstead(tabs, memory("a"), memory("b"), "B")).toBe(tabs);
  });
});
