import { $, browser, expect } from "@wdio/globals";

/**
 * **A vault in a tab of its own**, in the built app (charter-app#235): made from the palette's
 * New vault…, a secret added in its tab, and the table searched — through the real core, whose
 * `e2e` build is fenced and so keeps a keyring vault's values in `<state>/keyring-stub.json`
 * under the copied fixture plane, never in this machine's Keychain (`secrets::keyring`).
 *
 * What is proved here and not in jsdom: `vault_create` and the writes exist and answer off a
 * real plane, a keyring vault's keys index is what the table is drawn from, and **the value
 * typed is nowhere in the page** once it is written — asked of the built WebView's own document.
 *
 * **It leaves the window as it found it**, apart from the vault it made in the fixture's copy:
 * the tab it opens it closes. There is no command that deletes a vault, and the copy is this
 * run's.
 */

const TABS = '[role="tablist"][aria-label="Tabs"]';
const PALETTE = '[role="dialog"][aria-label="Command palette"]';
const VAULT = "e2e-vault";
const TAB = `[data-testid="vault-tab-${VAULT}"]`;
/** A value no other thing in the window could say. */
const VALUE = "e2e-value-3c9a71f0";

/** Runs a palette row by typing its words. Nothing clicks the palette. */
async function fromThePalette(words: string) {
  await browser.keys(["F2"]);
  await $(PALETTE).waitForDisplayed({ timeout: 20_000 });
  const box = await $("#palette-query");
  await browser.waitUntil(async () => box.isFocused(), {
    timeout: 10_000,
    timeoutMsg: "the palette did not take the keyboard when it opened",
  });
  await box.addValue(words);
  await browser.keys(["Enter"]);
}

/** The dialog titled `title`, once it is up. */
async function dialog(title: string) {
  const up = await $(`[role="dialog"][aria-labelledby]`);
  await browser.waitUntil(async () => (await up.getText()).includes(title), {
    timeout: 20_000,
    timeoutMsg: `the ${title} dialog never came up`,
  });
  return up;
}

/** The secrets' names, top to bottom, as the table draws them. */
async function secretNames(): Promise<string[]> {
  // No named helpers inside `execute`: the spec's bundler wraps them in a `__name` the page
  // does not have.
  return browser.execute(
    (tab: string) =>
      [...document.querySelectorAll(`${tab} tbody tr td:first-child`)].map((cell) =>
        (cell.textContent ?? "").trim(),
      ),
    TAB,
  );
}

/** Adds a secret through the tab's own Add. */
async function add(key: string, value: string) {
  await $(TAB).$("button=Add").click();
  const asking = await dialog(`Add a secret to ${VAULT}`);
  await asking.$("input:not([type])").setValue(key);
  await asking.$('input[type="password"]').setValue(value);
  await asking.$("button=Add secret").click();
  await browser.waitUntil(async () => (await secretNames()).includes(key), {
    timeout: 20_000,
    timeoutMsg: `${key} never appeared in the table`,
  });
}

// On the outermost describe, where WebdriverIO reads it before any runnable is built
// (`e2e/budget.test.ts`).
describe("a vault's tab", function () {
  this.timeout(180_000);

  after(async () => {
    const closer = await $(`${TABS} button[aria-label="Close ${VAULT}"]`);
    if (await closer.isExisting()) await closer.click();
  });

  it("is made from the palette's New vault…, and opens in front on an empty table", async () => {
    await fromThePalette("New vault");
    const asking = await dialog("New vault");
    await asking.$("input").setValue(VAULT);
    await asking.$("button=Create vault").click();

    const tab = await $(TAB);
    await tab.waitForExist({ timeout: 20_000 });
    await browser.waitUntil(async () => (await tab.getText()).includes("holds no secrets yet"), {
      timeout: 20_000,
      timeoutMsg: "the new vault's tab never said it was empty",
    });
    expect(await $(`${TABS} [role="tab"][aria-selected="true"]`).getText()).toBe(VAULT);
    expect(await tab.$("h2").getText()).toContain("keyring · 0 secrets");
  });

  it("adds a secret, and the value is nowhere in the page once it is written", async () => {
    await add("API_TOKEN", VALUE);
    await add("DB_URL", `${VALUE}-db`);

    expect(await secretNames()).toEqual(["API_TOKEN", "DB_URL"]);
    expect(await $(TAB).$("h2").getText()).toContain("2 secrets");
    const page = await browser.execute(() => {
      const fields = [...document.querySelectorAll("input")].map((field) => field.value);
      return `${document.documentElement.outerHTML}\n${fields.join("\n")}`;
    });
    expect(page).not.toContain(VALUE);
  });

  it("narrows the table to what is searched for", async () => {
    const search = await $(TAB).$('input[type="search"]');
    await search.setValue("api");

    await browser.waitUntil(async () => (await secretNames()).length === 1, {
      timeout: 20_000,
      timeoutMsg: "the search did not narrow the table",
    });
    expect(await secretNames()).toEqual(["API_TOKEN"]);

    await search.clearValue();
  });
});
