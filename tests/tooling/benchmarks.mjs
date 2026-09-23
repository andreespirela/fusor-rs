import assert from "node:assert/strict";
import { readFile, mkdir } from "node:fs/promises";
import { chromium, firefox, webkit } from "playwright";
import { createSiteServer, root } from "../../benchmarks/harness/server.mjs";
import { resolve } from "node:path";
import { frameworks } from "../../benchmarks/tools/lib/protocol.mjs";
const report = JSON.parse(
  await readFile(resolve(root, "apps/benchmarks/public/results.json"), "utf8"),
);
// Columns for the frameworks this report measured, in display order. A report
// from an earlier protocol hides later frameworks' columns.
const measured = frameworks.filter((framework) =>
  report.results.some((row) => row.framework === framework),
);
const server = createSiteServer();
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
let browser;
try {
  for (const name of (process.env.PLAYWRIGHT_BROWSERS || "chromium").split(
    ",",
  )) {
    browser = await { chromium, firefox, webkit }[name].launch(
      name === "chromium" && process.env.PLAYWRIGHT_CHANNEL
        ? { channel: process.env.PLAYWRIGHT_CHANNEL }
        : {},
    );
    const context = await browser.newContext({
      viewport: { width: 1440, height: 1000 },
    });
    const page = await context.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(origin + "/benchmarks/");
    await page.waitForFunction(
      () => document.querySelector(".load-status")?.hidden,
    );
    await page.locator(".run-meta").waitFor();
    assert.equal(await page.locator(".results thead th").count(), 8);
    assert.equal(
      await page.locator(".results thead th:not([hidden])").count(),
      measured.length + 1,
    );
    assert.equal(
      await page.locator(".results tbody tr").first().locator("td:not([hidden])").count(),
      measured.length,
    );
    const fusorHeader = await page.locator(".results thead th").nth(1).textContent();
    assert.match(fusorHeader, /fusor/);
    assert.doesNotMatch(fusorHeader, /unknown/);
    const historyLink = page.getByRole("link", {
      name: "Research history",
      exact: false,
    });
    assert.equal(
      await historyLink.getAttribute("href"),
      "/benchmarks/history.html",
    );
    const historyResponse = await page.request.get(
      origin + "/benchmarks/history.json",
    );
    assert.equal(historyResponse.status(), 200);
    const history = await historyResponse.json();
    assert(history.experiments.length > 0);
    const selected = history.experiments.find(
      (entry) => entry.id === history.published.experiment,
    );
    const recordResponse = await page.request.get(
      origin + `/benchmarks/history/${selected.id}/record.json`,
    );
    assert.equal(recordResponse.status(), 200);
    const record = await recordResponse.json();
    assert(record.notes.length > 0);
    const historyPage = await context.newPage();
    await historyPage.goto(origin + "/benchmarks/history.html");
    await historyPage
      .getByRole("link", { name: selected.title, exact: true })
      .click();
    await historyPage
      .getByRole("heading", { name: selected.title, exact: true })
      .waitFor();
    assert((await historyPage.locator("details").count()) > 0);
    await historyPage.close();

    assert.equal(
      await page.locator(".memory-table tbody tr").count(),
      report.memory.length,
    );
    await page.getByRole("button", { name: "Bundles", exact: true }).click();
    await page.waitForFunction(
      () => document.querySelectorAll(".results tbody tr").length === 3,
    );
    assert.match(
      await page.locator(".results tbody tr").first().textContent(),
      /Hello World/,
    );
    await page
      .getByRole("button", { name: "Keyed lists", exact: true })
      .click();
    await page.waitForFunction(
      () => document.querySelectorAll(".results tbody tr").length === 3,
    );
    assert.match(
      await page.locator(".results tbody").textContent(),
      /retained row identities checked/i,
    );
    const swaps = await page
      .locator(".results tbody tr")
      .filter({ hasText: "Swap two" })
      .locator("td:not([hidden]) .operation")
      .allTextContents();
    assert.deepEqual(
      swaps,
      measured.map(
        (framework) =>
          `${report.results.find((row) => row.framework === framework && row.id === "swap").mutations.moves} moves`,
      ),
    );
    await page.getByRole("button", { name: "P95", exact: true }).click();
    assert(
      await page
        .getByRole("button", { name: "P95", exact: true })
        .evaluate((node) => node.classList.contains("active")),
    );
    await page.getByRole("button", { name: "All tests", exact: true }).click();
    await page.getByRole("button", { name: "Median", exact: true }).click();
    if (name === "chromium") {
      await mkdir("target/screenshots", { recursive: true });
      await page.screenshot({
        path: "target/screenshots/benchmarks-desktop.png",
        fullPage: true,
      });
    }
    await page.setViewportSize({ width: 390, height: 844 });
    assert.equal(
      await page.locator(".mobile-table-hint").textContent(),
      `Swipe the table to compare all ${measured.length} frameworks →`,
    );
    assert(await page.locator(".mobile-table-hint").isVisible());
    // The comparison table scrolls inside its container instead of the page.
    assert(
      await page
        .locator(".results")
        .first()
        .evaluate((table) => table.parentElement.scrollWidth > table.parentElement.clientWidth),
    );
    assert(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    );
    if (name === "chromium")
      await page.screenshot({
        path: "target/screenshots/benchmarks-mobile.png",
        fullPage: true,
      });
    assert.deepEqual(errors, []);
    await context.close();
    const retry = await browser.newPage();
    let attempts = 0;
    await retry.route("**/benchmarks/results.json", (route) =>
      ++attempts === 1
        ? route.fulfill({ status: 503, body: "test retry" })
        : route.continue(),
    );
    await retry.goto(origin + "/benchmarks/");
    await retry
      .locator(".load-status")
      .filter({ hasText: "could not load" })
      .waitFor();
    await retry.getByRole("button", { name: "Retry report" }).click();
    await retry.waitForFunction(
      () => document.querySelector(".load-status")?.hidden,
    );
    await retry.locator(".run-meta").waitFor();
    assert.equal(attempts, 2);
    await retry.close();
    console.log(
      `PASS ${name}: recorded results, Rust filters/statistics, memory, mobile layout and failed-fetch retry`,
    );
    await browser.close();
    browser = null;
  }
} finally {
  await browser?.close();
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
}
