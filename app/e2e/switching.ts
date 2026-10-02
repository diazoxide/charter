import { realpathSync, renameSync } from "node:fs";
import { dirname, join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import type { Bench, Plan } from "../src/bench.ts";
import { anEmptyRecord, copyFixturePlane } from "./harness.js";
import { closeProject } from "./opening.js";
import { attributesOfEach } from "./reading.js";

/**
 * Ten projects open in one window, and the switch between them timed (FR-27, row L9 of the
 * spec's budgets of record). Shared by the scenario run, which records it on every pull request
 * (`specs/project-switch.e2e.ts`), and the benchmark, which holds it to the budget on the
 * operator's machine (`bench/projects.bench.ts`): one copy of how a project is opened and how
 * a switch is measured, so the two cannot drift into measuring different things.
 */

export const PROJECTS = '[role="tablist"][aria-label="Projects"]';

/** How many projects the window holds while it is measured: the acceptance's ten. */
export const OPEN = 10;

/** FR-27's budget, in milliseconds: switching among ten open projects. */
export const BUDGET_MS = 200;

/** What the app answered a command with, insisting it answered at all. */
export async function ask<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const answer = await browser.executeAsync(
    (
      name: string,
      passed: Record<string, unknown>,
      done: (out: { ok?: unknown; trouble?: string }) => void,
    ) => {
      void window.__TAURI__.core
        .invoke(name, passed)
        .then((ok) => done({ ok }))
        .catch((e: unknown) => done({ trouble: String(e) }));
    },
    command,
    args,
  );
  if (answer.trouble !== undefined) throw new Error(`${command} refused: ${answer.trouble}`);
  return answer.ok as T;
}

/**
 * Runs a job in the window and answers with its result, once it has finished — the scenario
 * run's, which has no benchmark display checks to pass first (`bench/window.ts` has those).
 */
export async function jobInTheWindow<T>(plan: Plan): Promise<T> {
  await browser.execute((plan) => (window.charterBench as Bench).begin(plan), plan);
  let state: { running: boolean; result?: unknown; trouble?: string } = { running: true };
  await browser.waitUntil(
    async () => {
      state = await browser.execute(() => (window.charterBench as Bench).poll());
      return !state.running;
    },
    { timeout: 60_000, interval: 50, timeoutMsg: `${plan.kind} never finished` },
  );
  if (state.trouble) throw new Error(`${plan.kind}: ${state.trouble}`);
  return state.result as T;
}

/** The project in front, by its path, as the strip says, read in one pass (`reading.ts`). */
export async function inFront(): Promise<string | null> {
  const tabs = await attributesOfEach(`${PROJECTS} [role="tab"]`, ["title", "aria-selected"]);
  return tabs.find((tab) => tab["aria-selected"] === "true")?.title ?? null;
}

/**
 * A project of the run's own, named `switch-<n>` so its tab, its `×` and its row say which it
 * is, with nothing to put back — so opening it starts nothing. Answered with the spelling the
 * app holds it by (`projects.e2e.ts` says why that is the resolved path).
 */
export function aProjectOfItsOwn(n: number, prepare: (plane: string) => void = () => {}): string {
  const copied = copyFixturePlane();
  const renamed = join(dirname(copied), `switch-${n}`);
  renameSync(copied, renamed);
  anEmptyRecord(renamed);
  prepare(renamed);
  return realpathSync(renamed);
}

/** Opens `plane` from the strip's `+`, through the trust gate, and waits for it in front. */
export async function openFromTheStrip(plane: string): Promise<void> {
  await $(`${PROJECTS} button[aria-label="Open a project…"]`).click();
  const box = await $("#open-by-path");
  await box.waitForDisplayed({ timeout: 20_000 });
  await box.addValue(plane);
  await $("button=Open").click();
  const question = await $('[role="dialog"]');
  await question.waitForDisplayed({ timeout: 30_000 });
  await expect(question).toHaveText("Open this project?", { containing: true });
  await $("button=Open project").click();
  await browser.waitUntil(async () => (await inFront()) === plane, {
    timeout: 30_000,
    timeoutMsg: `${plane} never came to the front`,
  });
}

/**
 * Closes every `switch-<n>` project by its `×`, answering the question a project with a chat
 * open asks. The strip collapses rather than scrolling, so a project it was hiding comes onto
 * it as the drawn ones go — `endEveryChat`'s loop one level up, and bounded the same way.
 */
export async function closeTheProjectsOfItsOwn(): Promise<void> {
  const closer = `${PROJECTS} button[aria-label^="Close project switch-"]`;
  for (let pressed = 0; pressed < OPEN; pressed++) {
    if (!(await $(closer).isExisting())) return;
    await closeProject(closer);
  }
  throw new Error(`${OPEN} presses did not close every project this run opened`);
}

function percentile(sorted: number[], at: number): number {
  return sorted[Math.min(sorted.length - 1, Math.floor((at / 100) * sorted.length))];
}

/** What `measureSwitches` found. */
export type Switches = {
  projects: number;
  samples: number;
  /** From the press of the row to the paint of the pane of the project's chat. */
  p50: number;
  /** L9's statistic (D-0009). */
  p95: number;
  p99: number;
  worst: number;
  /** From the press to the frame after the project is in front, before its pane has painted. */
  shownP50: number;
  /** From the switcher's key to the frame its row is drawn in: getting to the row. */
  openP50: number;
  openWorst: number;
  samples_ms: number[];
};

/**
 * Brings each project to the front `rounds` times, in the order given, through the switcher,
 * and answers with how long each took. The one in front when it starts must be the LAST one
 * given, so that no switch is ever to the project already there.
 *
 * @param sentinel What the chat in front in each project has printed, for its pane's paint.
 * @param job The window's own job runner, which the benchmark and the scenario run each have.
 */
export async function measureSwitches(
  planes: readonly string[],
  rounds: number,
  sentinel: (plane: string) => string,
  job: <T>(plan: Plan) => Promise<T>,
): Promise<Switches> {
  const painted: number[] = [];
  const shown: number[] = [];
  const opening: number[] = [];
  for (let round = 0; round < rounds; round++) {
    for (const plane of planes) {
      const took = await job<{ ms: number; shownMs: number; openMs: number }>({
        kind: "project switch",
        plane,
        sentinel: sentinel(plane),
      });
      expect(await inFront()).toBe(plane);
      painted.push(took.ms);
      shown.push(took.shownMs);
      opening.push(took.openMs);
    }
  }
  const sorted = [...painted].sort((a, b) => a - b);
  const opened = [...opening].sort((a, b) => a - b);
  return {
    projects: planes.length,
    samples: painted.length,
    p50: percentile(sorted, 50),
    p95: percentile(sorted, 95),
    p99: percentile(sorted, 99),
    worst: sorted[sorted.length - 1],
    shownP50: percentile(
      [...shown].sort((a, b) => a - b),
      50,
    ),
    openP50: percentile(opened, 50),
    openWorst: opened[opened.length - 1],
    samples_ms: painted,
  };
}
