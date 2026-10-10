/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { Notice, NoticeBand, NoticeOf, type NoticeProps } from "./Notice";
import * as set from "./settings/components";
import {
  Choice,
  Field,
  SettingActions,
  SettingGroup,
  SettingRow,
  SettingsLayout,
} from "./settings/components";
import { complaints } from "./theme/complaints.testkit";
import { BUILT_IN, DEFAULT_THEME, drawIn } from "./theme/theme";

/**
 * **The component set, drawn as one gallery** (DS-3 #626, line 2).
 *
 * The window has no story tool, and one would be a new dependency for what a rendered test
 * already does (D-626-1). So this is the gallery: every piece of the set, each in every kind it
 * has, drawn on one page, and held to three things:
 *
 * - **it is all here.** The pieces are read from `settings/components.tsx`'s exports, a field's
 *   and a choice's kinds from its source, and a Notice's places from its `at`. A piece or a
 *   kind added there and not drawn here fails;
 * - **its colours are tokens**, in each built-in theme: the rendered token check
 *   (`theme/complaints.testkit.ts`) that every view and dialog is held to;
 * - **the keyboard reaches and works it**: Tab visits every control in reading order, and
 *   Space, Enter, an arrow or typing does what a click would.
 *
 * The pieces' own behaviour, case by case, is in `settings/components.test.tsx`,
 * `Notice.pane.test.tsx` and `Notices.window.test.tsx`. This is the whole set at once.
 */

afterEach(cleanup);

/** What a control did, in the order it did it: `name` for a press, `name=value` for a pick. */
let did: string[] = [];
const said =
  (name: string) =>
  (...value: unknown[]) => {
    did.push(`${name}=${value.map(String).join(",")}`);
  };
/** A press: what it was handed (a click event, or nothing) is not what it did. */
const pressed = (name: string) => () => {
  did.push(name);
};

const OPTIONS = [
  { value: "a", label: "Alpha", says: "The first" },
  { value: "b", label: "Beta" },
  { value: "c", label: "Gamma", disabled: true, title: "Gamma is not installed" },
];

/** Every way out a Notice has, each saying it was pressed. */
const ways = (where: string) =>
  ({
    fixes: [
      { label: `Retry ${where}`, onPress: pressed(`retry ${where}`) },
      { label: `Undo ${where}`, onPress: pressed(`undo ${where}`) },
    ],
    link: { label: `Open settings ${where}`, onPress: pressed(`link ${where}`) },
    copy: `purlis doctor --${where}`,
    onDismiss: pressed(`dismiss ${where}`),
  }) satisfies Partial<NoticeProps>;

/** Every piece of the set, in every kind, as one page. */
function Gallery() {
  return (
    <>
      <SettingsLayout
        levels={[
          { id: "you", label: "You" },
          { id: "project", label: "Project" },
        ]}
        level="you"
        onLevelChange={said("level")}
        about="Your settings, on this machine."
        groups={[
          { id: "look", label: "Look" },
          { id: "keys", label: "Keys", sub: true },
        ]}
        group="look"
        onGroupChange={said("group")}
        filter=""
        onFilterChange={said("filter")}
        foot={<p>Kept in your settings file.</p>}
      >
        <SettingGroup label="Look" help="How the window is drawn.">
          <SettingRow
            label="Name"
            help="What the window calls you."
            reset={{ label: "Reset name", disabled: false, onReset: pressed("reset") }}
            undo={pressed("undo")}
            origin="Set in your settings file"
            badge="Overrides the project's"
            error={["purlis could not write it: the file is read-only"]}
            control={(ids) => (
              <Field
                ids={ids}
                kind="text"
                value=""
                placeholder="Ada"
                onChange={said("text")}
                onCommit={pressed("commit")}
              />
            )}
          />
          <SettingRow
            label="Hosts"
            control={(ids) => <Field ids={ids} kind="list" value="" onChange={said("list")} />}
          />
          <SettingRow
            label="Size"
            control={(ids) => (
              <Field
                ids={ids}
                kind="range"
                value={14}
                min={10}
                max={20}
                shown={(value) => `${value}px`}
                onChange={said("range")}
              />
            )}
          />
          <SettingRow
            label="Letter"
            grouped
            control={(ids) => (
              <Choice
                ids={ids}
                kind="radio"
                options={OPTIONS}
                value="a"
                onValueChange={said("radio")}
              />
            )}
          />
          <SettingRow
            label="Shell"
            control={(ids) => (
              <Choice
                ids={ids}
                kind="select"
                options={OPTIONS}
                value="a"
                unset="None"
                onValueChange={said("select")}
              />
            )}
          />
          <SettingRow
            label="Push after saving"
            control={(ids) => (
              <Choice ids={ids} kind="toggle" checked={false} onCheckedChange={said("toggle")} />
            )}
          />
          <SettingRow
            label="Repos"
            grouped
            control={(ids) => (
              <Choice
                ids={ids}
                kind="checks"
                options={OPTIONS}
                checked={new Set(["a"])}
                onCheckedChange={said("checks")}
              />
            )}
          />
          <SettingActions>
            <button type="button" tabIndex={0} onClick={pressed("save")}>
              Save
            </button>
            <button type="button" tabIndex={0} className="ends-it" onClick={pressed("delete")}>
              Delete
            </button>
          </SettingActions>
        </SettingGroup>
      </SettingsLayout>
      <NoticeBand>
        <Notice cause="gallery-band" tone="news" {...ways("band")}>
          A Notice under the strip.
        </Notice>
      </NoticeBand>
      <NoticeOf.Provider value={{ whose: "steward 2", onGo: pressed("go") }}>
        <Notice cause="gallery-pane" at="pane" tone="trouble" {...ways("pane")}>
          A Notice in a pane.
        </Notice>
      </NoticeOf.Provider>
      <Notice cause="gallery-drawer" at="drawer" tone="trouble" {...ways("drawer")}>
        A Notice in the Alerts drawer.
      </Notice>
    </>
  );
}

const source = (path: string) => readFileSync(join(process.cwd(), path), "utf8");

/** The kinds a union type in `path` spells as `kind: "…"`, between `type <name>` and its end. */
function kindsOf(path: string, type: string): string[] {
  const text = source(path);
  const from = text.indexOf(`export type ${type} =`);
  const to = text.indexOf("\n  );", from);
  expect(from, `${path} still declares ${type}`).toBeGreaterThan(-1);
  return [...text.slice(from, to).matchAll(/kind: "(\w+)"/g)].map((one) => one[1]);
}

describe("the gallery", () => {
  it("draws every piece the set exports", () => {
    const pieces = Object.entries(set)
      .filter(([name, value]) => typeof value === "function" && /^[A-Z]/.test(name))
      .map(([name]) => name);
    const drawn = source("src/gallery.test.tsx");
    expect(pieces.length).toBeGreaterThanOrEqual(6);
    expect(pieces.filter((name) => !new RegExp(`<${name}\\b`).test(drawn))).toEqual([]);
  });

  it("draws every kind of field and of choice", () => {
    render(<Gallery />);
    const fields = kindsOf("src/settings/components.tsx", "FieldProps");
    const choices = kindsOf("src/settings/components.tsx", "ChoiceProps");
    expect(fields).toEqual(["text", "list", "range"]);
    expect(choices).toEqual(["radio", "select", "toggle", "checks"]);
    // Each kind, found by what it is to a person.
    expect(screen.getByRole("textbox", { name: "Name" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Hosts" })).toBeInTheDocument();
    expect(screen.getByRole("slider", { name: "Size" })).toBeInTheDocument();
    expect(screen.getByRole("radiogroup", { name: "Letter" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Shell" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Push after saving" })).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Repos" })).toBeInTheDocument();
  });

  it("draws a Notice in every place it stands, each with every way out", () => {
    render(<Gallery />);
    const places = /at\?: ((?:"\w+"(?: \| )?)+);/.exec(source("src/Notice.tsx"))?.[1];
    expect(places?.match(/\w+/g)).toEqual(["band", "pane", "drawer"]);
    for (const where of ["band", "pane", "drawer"]) {
      const notice = document.querySelector<HTMLElement>(`[data-cause="gallery-${where}"]`);
      expect(notice, `a Notice at ${where}`).not.toBeNull();
      expect(notice).toHaveClass(`notice-${where}`);
      const buttons = within(notice as HTMLElement)
        .getAllByRole("button")
        .map((one) => one.textContent);
      expect(buttons).toEqual(
        expect.arrayContaining([
          `Retry ${where}`,
          `Undo ${where}`,
          `Open settings ${where}`,
          "Copy command",
          "Dismiss",
        ]),
      );
    }
    // A Notice of a chat the pane does not show says whose it is, with the way to it.
    expect(screen.getByRole("button", { name: "Go to it" })).toBeInTheDocument();
  });
});

describe.each(Object.keys(BUILT_IN))("the gallery in %s", (theme) => {
  beforeEach(() => drawIn(BUILT_IN[theme]));
  afterEach(() => drawIn(DEFAULT_THEME));

  it("holds no colour but a token's", () => {
    render(<Gallery />);
    expect(complaints(document.body)).toEqual([]);
  });
});

describe("the gallery by keyboard", () => {
  beforeEach(() => {
    did = [];
  });

  /** Every control Tab stops at, from the top, by its name. */
  async function tabOrder(user: ReturnType<typeof userEvent.setup>): Promise<string[]> {
    const stops: string[] = [];
    document.body.focus();
    for (let step = 0; step < 60; step += 1) {
      await user.tab();
      const at = document.activeElement;
      if (at === null || at === document.body) break;
      const name =
        at.getAttribute("aria-label") ??
        (at.id ? document.querySelector(`label[for="${at.id}"]`)?.textContent : null) ??
        at.textContent ??
        "";
      stops.push(`${at.getAttribute("role") ?? at.tagName.toLowerCase()} ${name.trim()}`);
    }
    return stops;
  }

  it("visits every control once, in reading order", async () => {
    const user = userEvent.setup();
    render(<Gallery />);
    const notice = (where: string) => [
      `button Retry ${where}`,
      `button Undo ${where}`,
      `button Open settings ${where}`,
      "button Copy command",
      "button Dismiss",
    ];
    expect(await tabOrder(user)).toEqual([
      // The level is one choice of a few, so one stop; the nav is one stop too.
      "radio You",
      "input Filter settings",
      "button Look",
      "input Name",
      "button Reset name",
      "button Undo",
      "textarea Hosts",
      "input Size",
      // A radio group is one stop, on its pick; Gamma is out of reach.
      "radio Alpha",
      "select Shell",
      "checkbox Push after saving",
      "checkbox Alpha",
      "checkbox Beta",
      "button Save",
      "button Delete",
      ...notice("band"),
      "button Go to it",
      ...notice("pane"),
      ...notice("drawer"),
    ]);
  });

  it("works every control with the keys a person would press", async () => {
    const user = userEvent.setup();
    render(<Gallery />);
    const press = async (
      role: Parameters<typeof screen.getByRole>[0],
      name: string,
      key: string,
    ) => {
      screen.getByRole(role, { name }).focus();
      await user.keyboard(key);
    };

    /** What `act` made the controls do, alone. */
    const doing = async (act: () => Promise<void>) => {
      did = [];
      await act();
      return did;
    };

    // A radio choice picks on a single arrow (`Choice`'s repair, docs/ui-primitives.md).
    expect(await doing(() => press("radio", "Alpha", "{ArrowDown}"))).toEqual(["radio=b"]);
    // The level switcher moves on an arrow and picks on Space, as Radix's own radio group does.
    expect(
      await doing(async () => {
        await press("radio", "You", "{ArrowRight}");
        expect(screen.getByRole("radio", { name: "Project" })).toHaveFocus();
        await user.keyboard(" ");
      }),
    ).toEqual(["level=project"]);
    // The nav goes to a group on Enter.
    expect(await doing(() => press("button", "Look", "{Enter}"))).toEqual(["group=look"]);
    // A field is typed into; a text field commits on Enter, and again when it is left.
    expect(await doing(() => press("searchbox", "Filter settings", "k"))).toEqual(["filter=k"]);
    expect(await doing(() => press("textbox", "Name", "A{Enter}"))).toEqual(["text=A", "commit"]);
    expect(await doing(() => press("textbox", "Hosts", "h"))).toEqual(["commit", "list=h"]);
    // A row's reset and Undo, on Space and on Enter.
    expect(await doing(() => press("button", "Reset name", " "))).toContain("reset");
    expect(await doing(() => press("button", "Undo", "{Enter}"))).toContain("undo");
    // A toggle and a box of a few, on Space.
    expect(await doing(() => press("checkbox", "Push after saving", " "))).toContain("toggle=true");
    expect(await doing(() => press("checkbox", "Beta", " "))).toEqual(["checks=b,true"]);
    // A form's buttons, on Enter and on Space.
    expect(await doing(() => press("button", "Save", "{Enter}"))).toEqual(["save"]);
    expect(await doing(() => press("button", "Delete", " "))).toEqual(["delete"]);
  });

  it("works every way out of a Notice, in every place, on Enter and on Space", async () => {
    const user = userEvent.setup();
    render(<Gallery />);
    const places = ["band", "pane", "drawer"];
    for (const [at, where] of places.entries()) {
      for (const label of [`Retry ${where}`, `Undo ${where}`, `Open settings ${where}`]) {
        screen.getByRole("button", { name: label }).focus();
        await user.keyboard("{Enter}");
      }
      screen.getAllByRole("button", { name: "Dismiss" })[at].focus();
      await user.keyboard(" ");
      screen.getAllByRole("button", { name: "Copy command" })[at].focus();
      await user.keyboard("{Enter}");
      expect(await navigator.clipboard.readText()).toBe(`purlis doctor --${where}`);
    }
    screen.getByRole("button", { name: "Go to it" }).focus();
    await user.keyboard(" ");

    expect(did).toEqual([
      ...places.flatMap((where) => [
        `retry ${where}`,
        `undo ${where}`,
        `link ${where}`,
        `dismiss ${where}`,
      ]),
      "go",
    ]);
  });
});
