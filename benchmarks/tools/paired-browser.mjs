// Supplemental alternating production controls using the fixed workload API.
import assert from "node:assert/strict";
import { chromium } from "playwright";
import { readFile, writeFile } from "node:fs/promises";
import { parseArgs } from "node:util";
import { sourceHash } from "../harness/provenance.mjs";
const { values: args } = parseArgs({
  options: Object.fromEntries(
    ["baseline", "candidate", "output"].map((key) => [key, { type: "string" }]),
  ),
});
if (!args.baseline || !args.candidate || !args.output)
  throw Error(
    "Use --baseline WORKLOAD_URL --candidate WORKLOAD_URL --output FILE",
  );
const output = args.output;
try {
  await readFile(output);
  throw Error("Refuse overwrite");
} catch (error) {
  if (error.code !== "ENOENT") throw error;
}
const origins = { baseline: args.baseline, candidate: args.candidate };
for (const url of Object.values(origins))
  if (!["http:", "https:"].includes(new URL(url).protocol))
    throw Error("Workload URLs must use HTTP(S)");
const report = {
  workingTreeSourceSha256: await sourceHash(),
  origins,
  provenance:
    "URLs identify served deployments, not their source. Preserve separate build receipts for both frozen deployments; workingTreeSourceSha256 identifies only the local checkout.",
  protocol:
    "Eight alternating preserved-baseline/candidate pairs; fresh context per variant; three warmups+seven samples for initial10k,bulk10k,fanout10k. Same workload APIs and operation+flush timing; full value/identity/nativeevent correctness outside timing. Not the publication evaluator.",
  pairs: [],
};
const browser = await chromium.launch({
  channel: process.env.PLAYWRIGHT_CHANNEL || "chrome",
});
try {
  for (let pair = 0; pair < 8; pair++) {
    const order =
        pair % 2 ? ["candidate", "baseline"] : ["baseline", "candidate"],
      result = { pair, order };
    for (const variant of order) {
      const context = await browser.newContext({
          viewport: { width: 1280, height: 800 },
        }),
        page = await context.newPage(),
        errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.goto(origins[variant]);
      await page.waitForFunction(() => globalThis.__benchReady !== undefined);
      const records = {};
      for (const operation of ["initial", "bulk", "fanout"]) {
        const samples = [];
        for (let run = 0; run < 10; run++) {
          const duration = await page.evaluate(async (operation) => {
            await __bench.unmount();
            await __bench.flush();
            if (operation !== "initial") {
              await __bench.mount(
                10000,
                operation === "fanout" ? "fanout" : "rows",
              );
              await __bench.flush();
            }
            const before =
              operation === "initial"
                ? []
                : [...document.querySelectorAll("#app li")];
            const start = performance.now();
            if (operation === "initial") await __bench.mount(10000, "rows");
            else if (operation === "bulk") __bench.bulk(10000);
            else __bench.fanout(11);
            await __bench.flush();
            const duration = performance.now() - start;
            const rows = [...document.querySelectorAll("#app li")],
              delta =
                operation === "bulk" ? 1 : operation === "fanout" ? 11 : 0;
            if (
              rows.length !== 10000 ||
              rows.some(
                (row, i) =>
                  row.dataset.id !== String(i) ||
                  row.querySelector(".value").textContent !==
                    String(i + delta) ||
                  (before.length && before[i] !== row),
              )
            )
              throw Error("control output/identity");
            rows[0].querySelector("button").click();
            await __bench.flush();
            if (
              rows[0].querySelector(".value").textContent !== String(delta + 1)
            )
              throw Error("control native event");
            return duration;
          }, operation);
          if (run >= 3) samples.push(duration);
        }
        records[operation] = {
          samples,
          median: [...samples].sort((a, b) => a - b)[3],
        };
      }
      assert.deepEqual(errors, []);
      result[variant] = { ...records, errors };
      await context.close();
    }
    report.pairs.push(result);
    await writeFile(output, JSON.stringify(report, null, 2) + "\n");
    console.log(JSON.stringify(result));
  }
  assert.equal(
    await sourceHash(),
    report.workingTreeSourceSha256,
    "source changed during control",
  );
} finally {
  await browser.close();
}
