import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * **Chrome is not selectable; content is** (SI-7). The operator's ruling: a drag or a
 * double-click on a tab, a row, a button or a caption must not leave text highlighted, and what
 * someone reads to copy — an error, a path, a view's answer, a dialog's words, a field — must
 * still select.
 *
 * jsdom computes no stylesheet, so this reads `App.css` as text (as `overscroll.test.ts` does)
 * and resolves `user-select` itself: every rule that says it is zero-specificity (`:where`), so
 * the last matching rule in the file decides an element, and an element no rule matches takes
 * its parent's — which is what `auto` computes to beside a `none` or `text` parent.
 */
const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
  /\/\*[\s\S]*?\*\//g,
  "",
);

interface Rule {
  selector: string;
  value: string;
}

/** Every rule in `App.css` that sets `property`, in source order, with the value it sets. */
function rulesSetting(property: string): Rule[] {
  const rules: Rule[] = [];
  for (const match of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const said = new RegExp(`(?:^|;)\\s*${property}:\\s*([a-z-]+)\\s*;`).exec(match[2]);
    if (said) rules.push({ selector: match[1].trim(), value: said[1] });
  }
  return rules;
}

function selectRules(): Rule[] {
  return rulesSetting("user-select");
}

/** What `user-select` resolves to on `element`, by the rules above. */
function selectable(element: Element): boolean {
  const rules = selectRules();
  for (let at: Element | null = element; at !== null; at = at.parentElement) {
    const here = rules.filter((rule) => at.matches(rule.selector)).at(-1);
    if (here !== undefined) return here.value === "text";
  }
  return true;
}

function draw(html: string): HTMLElement {
  document.body.innerHTML = html;
  return document.body;
}

function the(selector: string): Element {
  const found = document.querySelector(selector);
  if (found === null) throw new Error(`no ${selector} in the fixture`);
  return found;
}

describe("text selection", () => {
  it("is off for the whole window unless something opts in", () => {
    const root = selectRules().find((rule) => rule.selector === ":root");
    expect(root?.value).toBe("none");
  });

  it("is decided only by rules of no specificity, so the file's order is the whole cascade", () => {
    const others = selectRules().filter((rule) => rule.selector !== ":root");
    expect(others.length).toBeGreaterThan(0);
    for (const rule of others) expect(rule.selector).toMatch(/^:where\(/);
  });

  it("is not reset on every element, which would undo each opt-in for its children", () => {
    expect(selectRules().map((rule) => rule.selector)).not.toContain("*");
  });

  it("does not select the chrome: title bar, strips, rows, captions", () => {
    draw(`
      <header class="title-bar"><button role="tab"><span>charter</span></button>
        <span class="title-bar-doing">saving</span></header>
      <ul class="panel-rows"><li class="panel-row"><span class="row"><span class="row-text">steward</span></span></li></ul>
      <section class="view-pane"><header class="view-head"><h2>Memory</h2></header></section>
      <div class="pane"><div class="xterm"><div class="xterm-rows">$ ls</div></div></div>
    `);
    for (const at of [".title-bar span", ".title-bar-doing", ".row-text", ".view-head h2"]) {
      expect(selectable(the(at)), at).toBe(false);
    }
  });

  it("selects what is typed into a field", () => {
    draw(`<div role="dialog"><label>Name <input id="name"></label><textarea id="said"></textarea>
      <div contenteditable="true" id="edit">words</div></div>`);
    for (const at of ["#name", "#said", "#edit"]) expect(selectable(the(at)), at).toBe(true);
    expect(selectable(the("label")), "the field's label").toBe(false);
  });

  it("leaves the terminal's own input box to xterm", () => {
    draw(`<div class="xterm"><textarea class="xterm-helper-textarea"></textarea></div>`);
    expect(selectable(the(".xterm-helper-textarea"))).toBe(false);
  });

  it("selects a view's answer, but not the rows and buttons in it", () => {
    draw(`<div class="view-body">
      <p class="note">three memories</p>
      <dl class="facts"><div class="fact"><dt>vault</dt><dd id="value">none</dd></div></dl>
      <ul><li><button class="row"><span class="row-text">a memory</span></button></li></ul>
    </div>`);
    expect(selectable(the(".note"))).toBe(true);
    expect(selectable(the("#value"))).toBe(true);
    expect(selectable(the("button .row-text"))).toBe(false);
  });

  it("selects an error wherever it is said", () => {
    draw(`<aside class="panel"><p class="trouble">charter could not read it</p>
        <p class="honest" role="alert">the core went away</p></aside>
      <span class="alert-detail">git said no</span>`);
    for (const at of [".trouble", "[role=alert]", ".alert-detail"]) {
      expect(selectable(the(at)), at).toBe(true);
    }
  });

  it("selects a path or a name set as code, unless it sits in a control", () => {
    draw(`<footer class="status-line"><code id="plane">~/charter</code></footer>
      <button class="row"><code id="branch">main</code></button>
      <pre class="saving-said">pushed</pre>`);
    expect(selectable(the("#plane"))).toBe(true);
    expect(selectable(the(".saving-said"))).toBe(true);
    expect(selectable(the("#branch"))).toBe(false);
  });

  it("selects a dialog's or a card's words, but not its buttons", () => {
    draw(`<div role="dialog"><h2>About</h2><div class="about-body"><p>This is Charter 0.4.0.</p></div>
        <div class="answer"><button>Close</button></div></div>
      <div role="alertdialog"><p class="came-back">the core refused</p></div>`);
    expect(selectable(the(".about-body p"))).toBe(true);
    expect(selectable(the(".came-back"))).toBe(true);
    expect(selectable(the(".answer button"))).toBe(false);
  });

  it("selects a revealed vault value", () => {
    draw(`<div class="vault"><code class="vault-value">s3cr3t</code></div>`);
    expect(selectable(the(".vault-value"))).toBe(true);
  });

  it("does not drag a link or an image out as a ghost", () => {
    draw(`<a href="#">pr</a><img alt="">`);
    const drag = rulesSetting("-webkit-user-drag");
    for (const at of ["a", "img"]) {
      const here = drag.filter((rule) => the(at).matches(rule.selector)).at(-1);
      expect(here?.value, at).toBe("none");
    }
  });
});
