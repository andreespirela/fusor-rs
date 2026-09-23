// Behavioral regression checks for the shared keyed runtime. Timing is not scored.
import assert from "node:assert/strict";
import { chromium, firefox, webkit } from "playwright";
import { createSiteServer } from "../../benchmarks/harness/server.mjs";
const server = createSiteServer();
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
let browser;
try {
  for (const engine of (process.env.PLAYWRIGHT_BROWSERS || "chromium").split(
    ",",
  )) {
    browser = await { chromium, firefox, webkit }[engine].launch(
      engine === "chromium" && process.env.PLAYWRIGHT_CHANNEL
        ? { channel: process.env.PLAYWRIGHT_CHANNEL }
        : {},
    );
    const page = await browser.newPage();
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(origin + "/workloads/fusor/");
    await page.waitForFunction(() => globalThis.__benchReady !== undefined);
    const result = await page.evaluate(async () => {
      const api = globalThis.__bench;
      await api.mount(32, "rows");
      await api.flush();
      const original = [...document.querySelectorAll("#app li")];
      let order = original.map((_, i) => i),
        seed = 179,
        operations = 0;
      const check = () => {
        const rows = [...document.querySelectorAll("#app li")];
        if (
          rows.length !== order.length ||
          rows.some((row, i) => row !== original[order[i]])
        )
          throw Error("row identity/order changed");
      };
      for (let turn = 0; turn < 100; turn++) {
        seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
        const a = seed % order.length;
        seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
        const b = turn % 10 === 0 ? a : seed % order.length;
        const records = [];
        const observer = new MutationObserver((r) => records.push(...r));
        observer.observe(document.querySelector("#app ul"), {
          childList: true,
        });
        api.swap(a, b);
        await api.flush();
        await Promise.resolve();
        records.push(...observer.takeRecords());
        observer.disconnect();
        [order[a], order[b]] = [order[b], order[a]];
        check();
        const added = records
          .flatMap((r) => [...r.addedNodes])
          .filter((n) => n.nodeType === 1).length;
        const expected = a === b ? 0 : Math.abs(a - b) === 1 ? 1 : 2;
        if (added !== expected)
          throw Error(`swap ${a},${b}: ${added} moves, expected ${expected}`);
        operations++;
      }
      // A deletion retains every survivor and insertion creates exactly one new row.
      api.remove(15);
      await api.flush();
      order.splice(15, 1);
      check();
      api.insert(7, 500);
      await api.flush();
      const inserted = document.querySelector('[data-id="500"]');
      original[500] = inserted;
      order.splice(7, 0, 500);
      check();
      for (const id of order) {
        const row = original[id];
        row.querySelector("button").click();
      }
      await api.flush();
      for (const id of order)
        if (original[id].querySelector(".value").textContent !== String(id + 1))
          throw Error("event must increment exactly once");
      await api.unmount();
      await api.flush();
      if (document.querySelector("#app li"))
        throw Error("unmount retained rows");
      return { operations, retained: order.length };
    });
    assert.equal(result.operations, 100);
    assert.deepEqual(errors, []);
    console.log(
      `PASS ${engine}: 100 swaps use minimal moves, exact keyed identities, insert/delete, events and unmount`,
    );
    await browser.close();
    browser = null;
  }
} finally {
  await browser?.close();
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
}
