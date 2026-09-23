import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { gzipSync } from "node:zlib";
import { createHash } from "node:crypto";
import { asset } from "./server.mjs";
export async function bundle(page, framework, fixture) {
  const urls = await page.evaluate(() => [
    location.href,
    ...performance.getEntriesByType("resource").map((entry) => entry.name),
  ]);
  const files = [];
  for (const url of [...new Set(urls)]) {
    const pathname = new URL(url).pathname;
    if (!/\.(?:js|wasm|css)$/.test(pathname) && !pathname.endsWith("/"))
      continue;
    const path = asset(pathname);
    if (!path) continue;
    const data = await readFile(path);
    files.push({
      path: pathname,
      bytes: data.length,
      gzip: gzipSync(data, { level: 9 }).length,
      sha256: createHash("sha256").update(data).digest("hex"),
    });
  }
  assert(
    files.some((file) => file.path.endsWith("/")),
    "entry HTML was measured",
  );
  return {
    framework,
    fixture,
    files,
    raw: files.reduce((n, file) => n + file.bytes, 0),
    gzip: files.reduce((n, file) => n + file.gzip, 0),
  };
}
export async function measureBundles(context, origin, framework) {
  const bundles = [];
  for (const fixture of ["hello", "todo"]) {
    const page = await context.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    try {
      await page.goto(`${origin}/workloads/${framework}/${fixture}/`);
      await page.locator("h1").waitFor();
      if (fixture === "hello")
        assert.match(await page.locator("h1").textContent(), /Hello/i);
      else {
        await page.waitForFunction(
          () => document.querySelectorAll("li").length === 20,
        );
        await page.getByLabel("New task").fill("A measured task");
        await page
          .getByRole("button", { name: "Add task", exact: true })
          .click();
        await page.waitForFunction(
          () => document.querySelectorAll("li").length === 21,
        );
        await page.locator("li input[type=checkbox]").first().check();
        await page.waitForFunction(
          () =>
            document.querySelector(".task-count").textContent ===
            "21 tasks · 1 done",
        );
        await page.getByRole("button", { name: "done", exact: true }).click();
        await page.waitForFunction(
          () => document.querySelectorAll("li").length === 1,
        );
        await page
          .getByRole("button", { name: "Delete task", exact: true })
          .click();
        await page.waitForFunction(
          () =>
            document.querySelector(".task-count").textContent ===
            "20 tasks · 0 done",
        );
        await page.getByRole("button", { name: "active", exact: true }).click();
        await page.waitForFunction(
          () => document.querySelectorAll("li").length === 20,
        );
        assert.equal(
          await page.locator("li").last().locator("span").textContent(),
          "A measured task",
        );
      }
      assert.deepEqual(errors, []);
      bundles.push(await bundle(page, framework, fixture));
    } finally {
      await page.close();
    }
  }
  return bundles;
}
