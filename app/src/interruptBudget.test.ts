import { afterEach, describe, expect, it } from "vitest";
import {
  INTERRUPT_BUDGET,
  type Interrupts,
  countInterrupts as startCounting,
  withinTheBudget,
} from "./interruptBudget";

/**
 * The counter behind W10's interrupt budget (DS-9, #631), on a bare document: what it counts
 * as a prompt, and what it does not. The budget itself is held by the first run's scenario
 * tests (`FirstRun.test.tsx`).
 */

/** Every counter a test started, stopped after it so none watches the next test's page. */
const counting: Interrupts[] = [];
const countInterrupts = () => {
  const one = startCounting();
  counting.push(one);
  return one;
};

afterEach(() => {
  counting.splice(0).forEach((one) => one.stop());
  document.body.innerHTML = "";
});

/** Lets the observer see what the last lines put on the page. */
const settled = () => new Promise((resolve) => setTimeout(resolve, 0));

function shown(html: string): HTMLElement {
  const box = document.createElement("div");
  box.innerHTML = html;
  const one = box.firstElementChild as HTMLElement;
  document.body.append(one);
  return one;
}

describe("counting interrupts", () => {
  it("counts a dialog and an alert dialog, by the title that names them", async () => {
    const interrupts = countInterrupts();
    shown(`<div role="dialog" aria-labelledby="t1"><h2 id="t1">Start a chat</h2></div>`);
    await settled();
    shown(`<div role="alertdialog" aria-label="Delete this workspace?"></div>`);

    expect(interrupts.asked()).toEqual(["Start a chat", "Delete this workspace?"]);
  });

  it("counts an inline question: a group named by a question", () => {
    const interrupts = countInterrupts();
    shown(
      `<div role="group" aria-labelledby="q"><p id="q">Which forge are its repos on?</p>` +
        `<button>GitHub</button></div>`,
    );

    expect(interrupts.asked()).toEqual(["Which forge are its repos on?"]);
  });

  it("counts an open native dialog, by its own role", () => {
    const interrupts = countInterrupts();
    shown(`<dialog open aria-label="Open this project?"><button>Open project</button></dialog>`);

    expect(interrupts.asked()).toEqual(["Open this project?"]);
  });

  it("counts a fieldset whose legend is a question, and not one whose legend asks nothing", () => {
    const interrupts = countInterrupts();
    shown(
      `<fieldset><legend>Which forge are its repos on?</legend><button>GitHub</button></fieldset>`,
    );
    shown(`<fieldset><legend>Project template</legend><input type="radio" /></fieldset>`);

    expect(interrupts.asked()).toEqual(["Which forge are its repos on?"]);
  });

  it("counts a radio group named by a question, and not one that names a setting", () => {
    const interrupts = countInterrupts();
    shown(`<div role="radiogroup" aria-label="Which harness should start?"></div>`);
    shown(`<div role="radiogroup" aria-label="Project template"></div>`);

    expect(interrupts.asked()).toEqual(["Which harness should start?"]);
  });

  it("does not count the page, a status line, an alert or a group that asks nothing", () => {
    const interrupts = countInterrupts();
    shown(`<section aria-label="Open a repo to start"><input /></section>`);
    shown(`<p role="status">Copying your repo into its workspace…</p>`);
    shown(`<p role="alert">That is not a repo.</p>`);
    shown(`<div role="group" aria-label="Pane actions"><button>Split</button></div>`);

    expect(interrupts.asked()).toEqual([]);
  });

  it("counts a prompt asked twice as two, and one drawn again while shown as one", async () => {
    const interrupts = countInterrupts();
    const first = shown(`<div role="dialog" aria-label="Open this project?"></div>`);
    await settled();
    // React drawing it again in a new node, while the operator is still looking at it.
    first.replaceWith(first.cloneNode(true));
    await settled();
    expect(interrupts.asked()).toEqual(["Open this project?"]);

    document.body.innerHTML = "";
    await settled();
    shown(`<div role="dialog" aria-label="Open this project?"></div>`);

    expect(interrupts.asked()).toEqual(["Open this project?", "Open this project?"]);
  });

  it("counts a prompt that was already up when counting began", () => {
    shown(`<div role="dialog" aria-label="Start a chat"></div>`);

    expect(countInterrupts().asked()).toEqual(["Start a chat"]);
  });

  it("counts nothing after it is stopped", async () => {
    const interrupts = countInterrupts();
    shown(`<div role="dialog" aria-label="Start a chat"></div>`);
    await settled();
    interrupts.stop();
    shown(`<div role="alertdialog" aria-label="End this chat?"></div>`);

    expect(interrupts.asked()).toEqual(["Start a chat"]);
  });
});

describe("the budget", () => {
  it("is three prompts (W10)", () => {
    expect(INTERRUPT_BUDGET).toBe(3);
  });

  it("passes three prompts and refuses a fourth, naming every one", () => {
    const three = ["Which forge are its repos on?", "Open this project?", "Start a chat"];
    expect(() => withinTheBudget(three)).not.toThrow();

    expect(() => withinTheBudget([...three, "Sign in?"])).toThrow(
      "4 prompts before the first answered turn, over W10's budget of 3: " +
        "Which forge are its repos on? · Open this project? · Start a chat · Sign in?",
    );
  });
});
