import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { resolve, extname, sep } from "node:path";
import { chromium, expect } from "@playwright/test";
import { buildPackage } from "../../scripts/build.mjs";

await buildPackage("fusor-async-components");
const root = resolve("examples/async-components/dist");
const requests = [];
const server = createServer(async (request, response) => {
  if (request.url.startsWith("/api/")) {
    const entry = { url: request.url, response, closed: false };
    response.on("close", () => { entry.closed = true; });
    requests.push(entry);
    return;
  }
  const path = resolve(root, "." + (request.url === "/" ? "/index.html" : new URL(request.url, "http://localhost").pathname));
  if (!path.startsWith(root + sep)) { response.writeHead(404).end(); return; }
  try {
    const data = await readFile(path);
    response.setHeader("content-type", ({ ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm" })[extname(path)] || "text/plain");
    response.end(data);
  } catch { response.writeHead(404).end(); }
});
await new Promise(done => server.listen(0, "127.0.0.1", done));
const complete = (kind, key, status = 200) => {
  const entry = requests.findLast(entry => entry.url === `/api/${kind}/${key}` && !entry.closed);
  assert(entry, `missing request ${kind}/${key}`);
  entry.response.writeHead(status).end(`${kind}-${key}`);
};
const waitRequests = async count => expect.poll(() => requests.length).toBe(count);
let browser;
try {
  browser = await chromium.launch(process.env.PLAYWRIGHT_CHANNEL ? { channel: process.env.PLAYWRIGHT_CHANNEL } : {});
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${server.address().port}`);
  await waitRequests(4);
  await expect(page.locator("async, await")).toHaveCount(0);
  await expect(page.locator("#independent > article")).toHaveCount(2);
  const price = page.locator("#independent > article").nth(0);
  const stock = page.locator("#independent > article").nth(1);
  complete("price", "A");
  await expect(price.locator(".value")).toHaveText("price-A");
  await expect(price).toHaveAttribute("data-response", "price-A");
  await expect(stock.locator(".value")).toHaveText("");
  await expect(price.locator(".nested")).toHaveText("price-A / price-A");
  await expect(price.locator(".supplied")).toHaveText("price-A");
  await price.locator(".use").click();
  await expect(page.locator("#clicked")).toHaveText("price-A");
  complete("group-price", "A");
  await expect(page.locator("#together .value")).toHaveCount(0);
  complete("group-stock", "A");
  await expect(page.locator("#together .value")).toHaveText(["group-price-A", "group-stock-A"]);
  complete("stock", "A");
  await expect(stock.locator(".value")).toHaveText("stock-A");
  await price.locator(".value").evaluate(node => { window.originalPrice = node; });

  await page.locator("#b").click();
  await waitRequests(8);
  complete("price", "B");
  await expect(price.locator(".value")).toHaveText("price-B");
  await expect(price).toHaveAttribute("data-response", "price-B");
  await expect(price.locator(".value")).toHaveAttribute("title", "price-B");
  await expect(price.locator(".supplied")).toHaveText("price-B");
  assert(await price.locator(".value").evaluate(node => node === window.originalPrice));
  complete("stock", "B", 503);
  complete("group-price", "B");
  complete("group-stock", "B", 503);
  await expect(stock).toHaveAttribute("inert", "");
  await expect(stock.locator(".value")).toHaveText("stock-A");
  await stock.locator(".use").evaluate(node => node.click());
  await expect(stock).toHaveAttribute("data-response", "stock-A");
  await expect(page.locator("#clicked")).toHaveText("price-A");
  await expect(page.locator("#together h2")).toHaveText("A");

  await page.locator("#c").click();
  await waitRequests(12);
  for (const kind of ["price", "stock", "group-price", "group-stock"]) complete(kind, "C");
  await expect(page.locator("#together h2")).toHaveText("C");
  await expect(price.locator(".nested")).toHaveText("price-C / price-C");
  await expect(price.locator("li")).toHaveText("price-C / price-C");
  await price.locator(".use").click();
  await expect(page.locator("#clicked")).toHaveText("price-C");

  await page.locator("#b").click();
  await waitRequests(16);
  const pending = requests.slice(12);
  await page.locator("#toggle").click();
  await expect(page.locator("#independent > article")).toHaveCount(1);
  await expect.poll(() => pending.find(entry => entry.url === "/api/price/B").closed).toBe(true);
  await page.locator("#toggle").click();
  await waitRequests(17);
  complete("price", "B");
  await expect(price.locator(".value")).toHaveText("price-B");
  await page.locator("#retained-toggle").click();
  await waitRequests(18);
  const removedRead = requests.at(-1);
  await page.locator("#retained-toggle").click();
  await expect.poll(() => removedRead.closed).toBe(true);
  await page.locator("#retained-toggle").click();
  await waitRequests(19);
  complete("retained", "value");
  await expect(page.locator("#retained-value")).toHaveText("retained-value");

  // get_text reports cancellation the same way before and during a request.
  await page.evaluate(async () => {
    const script = document.querySelector('script[type="module"]');
    window.api = await import(new URL("./pkg/app.js", script.src));
  });
  const sent = requests.length;
  assert.equal(await page.evaluate(() => window.api.probe_get_text("/api/probe/before", true)), "cancelled: request cancelled");
  assert.equal(requests.length, sent);
  await page.evaluate(() => { window.during = window.api.probe_get_text("/api/probe/during", false); });
  await waitRequests(sent + 1);
  await page.evaluate(() => window.api.cancel_probe());
  assert.equal(await page.evaluate(() => window.during), "cancelled: request cancelled");
  await expect.poll(() => requests.at(-1).closed).toBe(true);
  await page.evaluate(() => { window.missing = window.api.probe_get_text("/api/probe/missing", false); });
  await waitRequests(sent + 2);
  complete("probe", "missing", 404);
  assert.equal(await page.evaluate(() => window.missing), "status: GET /api/probe/missing: HTTP 404");
  await page.evaluate(() => { window.found = window.api.probe_get_text("/api/probe/found", false); });
  await waitRequests(sent + 3);
  complete("probe", "found");
  assert.equal(await page.evaluate(() => window.found), "ok: probe-found");
  await page.evaluate(async () => {
    const script = document.querySelector('script[type="module"]');
    const api = await import(new URL("./pkg/app.js", script.src));
    api.stop();
  });
  await expect.poll(() => pending.every(entry => entry.closed)).toBe(true);
  assert.deepEqual(errors, []);
  console.log("PASS Async/Await: independent completion, automatic grouping, named/nested values, events, Children, error retention, recovery, identity, cancellation, remount and get_text outcomes");
} finally {
  await browser?.close();
  server.closeAllConnections();
  await new Promise(done => server.close(done));
}
