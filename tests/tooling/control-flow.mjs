import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { resolve, extname, sep } from "node:path";
import { chromium, firefox, webkit, expect } from "@playwright/test";
import { buildPackage, command } from "../../scripts/build.mjs";
await buildPackage("fusor-control-flow");
const rendered = await command(
  "cargo",
  ["run", "-q", "-p", "fusor-control-flow", "--bin", "render"],
  { capture: true },
);
const root = resolve("examples/control-flow/dist");
const pending = new Map();
const server = createServer(async (req, res) => {
  const path = new URL(req.url, "http://localhost").pathname;
  if (path.startsWith("/api/")) {
    pending.set(path, [...(pending.get(path) || []), res]);
    return;
  }
  const file = resolve(root, path === "/" ? "index.html" : path.slice(1));
  if (!file.startsWith(root + sep)) {
    res.writeHead(404).end();
    return;
  }
  try {
    res.setHeader(
      "content-type",
      {
        ".html": "text/html",
        ".js": "text/javascript",
        ".wasm": "application/wasm",
      }[extname(file)] || "text/plain",
    );
    res.end(await readFile(file));
  } catch {
    res.writeHead(404).end();
  }
});
await new Promise((done) => server.listen(0, "127.0.0.1", done));
let browser;
let activePage;
try {
  for (const name of (process.env.PLAYWRIGHT_BROWSERS || "chromium").split(
    ",",
  )) {
    browser = await { chromium, firefox, webkit }[name].launch();
    const page = await browser.newPage();
    activePage = page;
    const errors = [];
    page.on("pageerror", (e) => {
      errors.push(e.message);
    });
    page.on("console", (e) => {
      if (e.type() === "error") {
        errors.push(e.text());
      }
    });
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await expect(page.locator("#guest-state")).toHaveText(
      "Sign in to continue.",
    );
    await expect(page.locator("#rows li")).toHaveText(["First", "2"]);
    await expect(page.locator("#option-value")).toHaveText("7");
    await expect(page.locator("#projected")).toHaveText("Projected children");
    await expect(page.locator("if,else,match,case")).toHaveCount(0);
    await page.locator("#draft").fill("keep my draft");
    await page.locator("#login").click();
    await expect(page.locator("#greeting")).toHaveText("Welcome, Ada");
    await expect(page.locator("#coherent")).toHaveText("Waiting");
    await expect.poll(() => pending.get("/api/Ada")?.length === 2).toBe(true);
    for (const response of pending.get("/api/Ada"))
      response.end("Ada response");
    pending.delete("/api/Ada");
    await expect(page.locator("#coherent .dashboard p")).toHaveText("Ada");
    await expect(page.locator("#outer-response")).toHaveText("Ada response");
    await page.locator("#outer-draft").fill("retained through loading");
    await page.locator("#session .count").click();
    await page.locator("#coherent .count").click();
    await page
      .locator("#session .dashboard")
      .evaluate((el) => (window.dashboard = el));
    await page
      .locator("#coherent .dashboard")
      .evaluate((el) => (window.coherent = el));
    await page.locator("#rename").click();
    await expect(page.locator("#greeting")).toHaveText("Welcome, Grace");
    await expect(page.locator("#session .dashboard p")).toHaveText("Grace");
    await expect(page.locator("#coherent .dashboard p")).toHaveText("Ada");
    await expect(page.locator("#coherent .response")).toHaveText(
      "Ada response",
    );
    await expect.poll(() => pending.get("/api/Grace")?.length === 2).toBe(true);
    for (const response of pending.get("/api/Grace"))
      response.end("Grace response");
    pending.delete("/api/Grace");
    await expect(page.locator("#coherent .dashboard p")).toHaveText("Grace");
    await expect(page.locator("#coherent .response")).toHaveText(
      "Grace response",
    );
    await expect(page.locator("#outer-response")).toHaveText("Grace response");
    await expect(page.locator("#nested-response")).toHaveText("Grace response");
    await expect(page.locator("#outer-draft")).toHaveValue(
      "retained through loading",
    );
    await page.locator("#outer-capture").click();
    await expect(page.locator("#clicked")).toHaveText("Grace response");
    await expect(page.locator("#session .count")).toHaveText("1");
    await expect(page.locator("#coherent .count")).toHaveText("1");
    assert(
      await page
        .locator("#session .dashboard")
        .evaluate((el) => el === window.dashboard),
    );
    assert(
      await page
        .locator("#coherent .dashboard")
        .evaluate((el) => el === window.coherent),
    );
    await expect(page.locator("#draft")).toHaveValue("keep my draft");
    await expect(page.locator("#nested")).toHaveText("Grace");
    await expect(page.locator("#session li")).toHaveText([
      "Grace 1",
      "Grace 2",
    ]);
    await page.locator("#capture").click();
    await expect(page.locator("#clicked")).toHaveText("Grace");
    await page.locator("#toggle").click();
    await expect(page.locator("#draft")).toHaveCount(0);
    await expect(page.locator("#hidden")).toHaveText("Hidden");
    await expect(page.locator("#optional")).toBeVisible();
    await expect(page.locator("#nested")).toHaveCount(0);
    await page.locator("#toggle").click();
    await expect(page.locator("#draft")).toHaveValue("");
    await expect(page.locator("#optional")).toHaveCount(0);
    const retired = await page.locator("#session #capture").elementHandle();
    await page.locator("#guest").click();
    await expect(page.locator("#session .dashboard")).toHaveCount(0);
    await expect(page.locator("#coherent .dashboard")).toHaveCount(0);
    await retired.evaluate((el) => el.click());
    await expect(page.locator("#clicked")).toHaveText("");
    await page.locator("#login").click();
    await expect(page.locator("#session .count")).toHaveText("0");
    await expect.poll(() => pending.get("/api/Ada")?.length === 2).toBe(true);
    for (const response of pending.get("/api/Ada"))
      response.end("Ada response");
    pending.delete("/api/Ada");
    await expect(page.locator("#coherent .count")).toHaveText("0");
    await page.locator("#checking").click();
    await expect(page.locator("#checking-state")).toBeVisible();
    // Adopt real server output, including an empty false branch and a nested component.
    await page.evaluate(async (html) => {
      document.body.insertAdjacentHTML("beforeend", html);
      window.serverDashboard = document.querySelector("#shared .dashboard");
      const boot = document.querySelector('script[type="module"]').src;
      window.client = await import(new URL("./pkg/app.js", boot).href);
      window.client.hydrate_shared(false);
    }, rendered);
    assert(
      await page
        .locator("#shared .dashboard")
        .evaluate((el) => el === window.serverDashboard),
    );
    await page.locator("#shared .count").click();
    await expect(page.locator("#shared .count")).toHaveText("1");
    await page.evaluate(() => window.client.stop());
    assert.deepEqual(errors, []);
    await browser.close();
    browser = null;
    console.log(
      `${name}: conditional branches, reactive captures, ownership, coherent rendering and hydration passed`,
    );
  }
} catch (error) {
  console.error(
    "Boundary",
    await activePage?.locator("#boundary-status").textContent(),
  );
  console.error(
    "Pending requests",
    [...pending].map(([key, value]) => [key, value.length]),
  );
  throw error;
} finally {
  await browser?.close();
  for (const response of [...pending.values()].flat()) response.end("disposed");
  server.closeAllConnections();
  await new Promise((done) => server.close(done));
}
