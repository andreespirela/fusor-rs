// Named-Wasm diagnostic only; no instrumented time enters benchmark results.
import assert from "node:assert/strict";
import { dirname } from "node:path";
import { chromium } from "playwright";
import { createSiteServer } from "../harness/server.mjs";
import { sourceHash } from "../harness/provenance.mjs";
import { mkdir, writeFile } from "node:fs/promises";
const output = process.argv[2];
if (!output)
  throw Error(
    "Usage: node benchmarks/tools/profile-updates.mjs OUTPUT_DIRECTORY",
  );
// Each invocation owns a fresh directory; existing evidence is never overwritten.
await mkdir(dirname(output), { recursive: true });
await mkdir(output);
const server = createSiteServer();
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const browser = await chromium.launch({
  channel: process.env.PLAYWRIGHT_CHANNEL || "chrome",
});
const report = {
  sourceSha256: await sourceHash(),
  browser: browser.version(),
  phases: {},
  errors: [],
  limitations:
    "Currently served Wasm build; preserve its build receipt and retain Wasm names when building for attribution. Sampling affects engine tiering. Eight profiles per operation, each20updates after3warmups. Includes CDP/evaluator gaps; reports all weights including program/idle. This is attribution, not production timing.",
};
try {
  for (const phase of ["bulk", "fanout"]) {
    const page = await browser.newPage();
    page.on("pageerror", (error) => report.errors.push(error.message));
    await page.goto(
      `http://127.0.0.1:${server.address().port}/workloads/fusor/`,
    );
    await page.waitForFunction(() => globalThis.__benchReady !== undefined);
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Profiler.enable");
    await cdp.send("Profiler.setSamplingInterval", { interval: 100 });
    const self = new Map(),
      inclusive = new Map();
    let totalWeight = 0,
      samples = 0;
    for (let round = 0; round < 8; round++) {
      await page.evaluate(async (phase) => {
        await __bench.unmount();
        await __bench.mount(10000, phase === "bulk" ? "rows" : "fanout");
        for (let i = 1; i <= 3; i++) {
          await __bench[phase](phase === "bulk" ? 10000 : i);
          await __bench.flush();
        }
      }, phase);
      await cdp.send("Profiler.start");
      await page.evaluate(async (phase) => {
        for (let i = 4; i <= 23; i++) {
          await __bench[phase](phase === "bulk" ? 10000 : i);
          await __bench.flush();
        }
      }, phase);
      const { profile } = await cdp.send("Profiler.stop");
      await writeFile(
        `${output}/${phase}-${round}.cpuprofile`,
        JSON.stringify(profile),
        { flag: "wx" },
      );
      await page.evaluate(() => {
        const rows = [...document.querySelectorAll("#app li")];
        if (
          rows.length !== 10000 ||
          rows.some(
            (row, i) =>
              row.querySelector(".value").textContent !== String(i + 23),
          )
        )
          throw Error("diagnostic values");
      });
      const nodes = new Map(profile.nodes.map((node) => [node.id, node])),
        parents = new Map();
      for (const node of profile.nodes)
        for (const child of node.children || []) parents.set(child, node.id);
      const key = (node) =>
        `${node.callFrame.functionName || "(anonymous)"} @ ${node.callFrame.url || "(native)"}`;
      for (let i = 0; i < (profile.samples || []).length; i++) {
        const weight = profile.timeDeltas?.[i] || 0,
          id = profile.samples[i];
        totalWeight += weight;
        samples++;
        self.set(
          key(nodes.get(id)),
          (self.get(key(nodes.get(id))) || 0) + weight,
        );
        const seen = new Set();
        for (
          let cursor = id;
          cursor !== undefined;
          cursor = parents.get(cursor)
        ) {
          const name = key(nodes.get(cursor));
          if (!seen.has(name)) {
            inclusive.set(name, (inclusive.get(name) || 0) + weight);
            seen.add(name);
          }
        }
      }
    }
    const table = (map) =>
      [...map]
        .sort((a, b) => b[1] - a[1])
        .map(([frame, weight]) => ({
          frame,
          sampledMicroseconds: weight,
          fraction: weight / totalWeight,
        }));
    report.phases[phase] = {
      samples,
      totalSampleWeightMicroseconds: totalWeight,
      self: table(self),
      inclusive: table(inclusive),
    };
    await page.close();
  }
  assert.deepEqual(report.errors, []);
  await writeFile(
    `${output}/summary.json`,
    JSON.stringify(report, null, 2) + "\n",
    { flag: "wx" },
  );
} finally {
  await browser.close();
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
}
