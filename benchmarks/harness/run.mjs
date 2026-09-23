import { chromium } from "playwright";
import { createSiteServer, root } from "./server.mjs";
import { readFile, writeFile, mkdir, rename } from "node:fs/promises";
import { resolve } from "node:path";
import { bundle, measureBundles } from "./bundles.mjs";
import { sourceHash } from "./provenance.mjs";
import os from "node:os";
import { execFileSync } from "node:child_process";
import { current } from "../tools/lib/protocol.mjs";
const samples = Number(process.env.BENCH_SAMPLES || 15),
  warmups = Number(process.env.BENCH_WARMUPS || 3);
const outputDirectory = resolve(
  root,
  process.env.BENCH_OUTPUT_DIR || "target/benchmarks/current",
);
const frameworks = (
  process.env.BENCH_FRAMEWORKS || current.frameworks.join(",")
).split(",");
if (
  new Set(frameworks).size !== frameworks.length ||
  frameworks.some((framework) => !current.frameworks.includes(framework))
)
  throw Error(`BENCH_FRAMEWORKS must name distinct frameworks of ${current.id}`);
const seed = Number(process.env.BENCH_SEED || 20260919);
let random = seed >>> 0;
for (let i = frameworks.length - 1; i > 0; i--) {
  random = (Math.imul(random, 1664525) + 1013904223) >>> 0;
  const j = random % (i + 1);
  [frameworks[i], frameworks[j]] = [frameworks[j], frameworks[i]];
}
if (
  !Number.isInteger(samples) ||
  samples < 1 ||
  !Number.isInteger(warmups) ||
  warmups < 0
)
  throw Error("Invalid sample or warmup count");
const definitions = [
  ["initial-1000", "Initial render · 1,000", "render", 1000, "rows", "mount"],
  [
    "initial-10000",
    "Initial render · 10,000",
    "render",
    10000,
    "rows",
    "mount",
  ],
  ["single", "Single update · 10,000", "updates", 10000, "rows", "single"],
  [
    "bulk-1000",
    "Bulk update · 1,000 of 10,000",
    "updates",
    10000,
    "rows",
    "bulk",
    1000,
  ],
  [
    "bulk-10000",
    "Bulk update · 10,000",
    "updates",
    10000,
    "rows",
    "bulk",
    10000,
  ],
  ["insert", "Insert a keyed row · 10,000", "lists", 10000, "rows", "insert"],
  ["delete", "Delete a keyed row · 10,000", "lists", 10000, "rows", "remove"],
  ["swap", "Swap two keyed rows · 10,000", "lists", 10000, "rows", "swap"],
  [
    "computed",
    "Derived state → DOM · 10,000",
    "graph",
    10000,
    "computed",
    "single",
  ],
  ["fanout-1000", "One source → 1,000 rows", "graph", 1000, "fanout", "fanout"],
  [
    "fanout-10000",
    "One source → 10,000 rows",
    "graph",
    10000,
    "fanout",
    "fanout",
  ],
  ["fanin", "10,000 inputs → one sum", "graph", 10000, "fanin", "fanin"],
  ["unmount", "Unmount · 10,000", "lifecycle", 10000, "rows", "unmount"],
  ["events", "Dispatch 10,000 click events", "events", 10000, "rows", "events"],
];
const server = createSiteServer();
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({
  channel: process.env.PLAYWRIGHT_CHANNEL || "chrome",
});
const packages = JSON.parse(
  await readFile(resolve(root, "benchmarks/package.json"), "utf8"),
);
const ssr = JSON.parse(
  await readFile(resolve(outputDirectory, "ssr.json"), "utf8"),
);
const build = JSON.parse(
  await readFile(resolve(root, "benchmarks/dist/build.json"), "utf8"),
);
if (build.schema !== 2) throw Error("Rebuild with `just bench-build`");
const report = {
  schema: 2,
  protocol: current.id,
  // Only the complete current protocol is a full run; any subset is a smoke test.
  profile:
    samples === current.samples &&
    warmups === current.warmups &&
    !process.env.BENCH_SKIP_MEMORY &&
    frameworks.length === current.frameworks.length
      ? "baseline"
      : "smoke",
  generatedAt: new Date().toISOString(),
  sourceSha256: await sourceHash(),
  environment: {
    seed,
    frameworkOrder: frameworks,
    browser: browser.version(),
    os: `${os.platform()} ${os.release()}`,
    arch: os.arch(),
    cpu: os.cpus()[0]?.model,
    logicalCpus: os.cpus().length,
    totalMemory: os.totalmem(),
    node: process.version,
    rust: execFileSync("rustc", ["--version"], { encoding: "utf8" }).trim(),
    samples,
    warmups,
    throttling: "none",
    isolation:
      "sequential frameworks; fresh browser contexts; production builds",
  },
  versions: {
    "fusor": build.toolchain.fusor.version,
    ...packages.dependencies,
    "leptos": build.toolchain.leptos.leptos,
  },
  // Rust/Wasm production pipeline settings recorded by `just bench-build`.
  toolchain: build.toolchain,
  results: [
    ...ssr.records.filter((record) => frameworks.includes(record.framework)),
  ],
  memory: [],
  bundles: [],
  errors: [],
};
function stats(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return {
    median: sorted[Math.floor(sorted.length / 2)],
    p95: sorted[
      Math.min(sorted.length - 1, Math.ceil(sorted.length * 0.95) - 1)
    ],
    min: sorted[0],
    max: sorted.at(-1),
  };
}
async function collect(cdp, page) {
  await page.evaluate(
    () =>
      new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      ),
  );
  await cdp.send("HeapProfiler.collectGarbage");
  await cdp.send("HeapProfiler.collectGarbage");
  return {
    heap: await cdp.send("Runtime.getHeapUsage"),
    dom: await cdp.send("Memory.getDOMCounters"),
    wasmCapacity: await page.evaluate(() => __bench.wasmBytes?.() || 0),
  };
}
try {
  for (const framework of frameworks) {
    console.log(`Measuring ${framework}`);
    const context = await browser.newContext({
      viewport: { width: 1280, height: 800 },
    });
    const page = await context.newPage();
    page.setDefaultTimeout(90000);
    const pageErrors = [];
    page.on("pageerror", (error) => pageErrors.push(error.message));
    await page.goto(`${origin}/workloads/${framework}/`);
    await page.waitForFunction(() => globalThis.__benchReady !== undefined);
    report.bundles.push(await bundle(page, framework, "benchmark workload"));
    try {
      report.bundles.push(
        ...(await measureBundles(context, origin, framework)),
      );
    } catch (error) {
      report.errors.push({ framework, id: "bundles", error: error.message });
      console.error("Bundle fixture failed", framework, error.message);
    }
    for (const [
      id,
      label,
      category,
      n,
      mode,
      operation,
      count,
    ] of definitions) {
      const values = [];
      let mutations;
      try {
        for (let run = 0; run < warmups + samples; run++) {
          const result = await page.evaluate(
            async ({ n, mode, operation, count }) => {
              const api = globalThis.__bench;
              if (document.querySelector("#app section")) {
                await api.unmount();
                await api.flush();
              }
              if (operation !== "mount") {
                await api.mount(n, mode);
                await api.flush();
              }
              const structural = ["insert", "remove", "swap"].includes(
                operation,
              );
              const before = structural
                ? [...document.querySelectorAll("#app li")]
                : [];
              const buttons =
                operation === "events"
                  ? [...document.querySelectorAll("#app button")]
                  : [];
              const observed = [];
              const observer = structural
                ? new MutationObserver((records) => observed.push(...records))
                : null;
              observer?.observe(document.querySelector("#app ul"), {
                childList: true,
              });
              const start = performance.now();
              switch (operation) {
                case "mount":
                  await api.mount(n, mode);
                  break;
                case "unmount":
                  await api.unmount();
                  break;
                case "single":
                  api.update(5000, 5001);
                  break;
                case "bulk":
                  api.bulk(count);
                  break;
                case "insert":
                  api.insert(5000, n);
                  break;
                case "remove":
                  api.remove(5000);
                  break;
                case "swap":
                  api.swap(1, n - 2);
                  break;
                case "fanout":
                  api.fanout(11);
                  break;
                case "fanin":
                  api.fanin();
                  break;
                case "events":
                  for (const button of buttons) button.click();
                  break;
              }
              await api.flush();
              const ms = performance.now() - start;
              const records = [...observed, ...(observer?.takeRecords() || [])];
              observer?.disconnect();
              const rows = [...document.querySelectorAll("#app li")];
              const expectedCount =
                operation === "unmount" || mode === "fanin"
                  ? 0
                  : n +
                    (operation === "insert"
                      ? 1
                      : operation === "remove"
                        ? -1
                        : 0);
              if (rows.length !== expectedCount)
                throw Error(
                  `row count ${rows.length}, expected ${expectedCount}`,
                );
              if (mode === "fanin") {
                const sum = (n * (n - 1)) / 2 + n;
                if (
                  document.querySelector("#total").textContent !== String(sum)
                )
                  throw Error("fan-in sum mismatch");
              }
              for (let index = 0; index < rows.length; index++) {
                let id = index;
                if (operation === "insert")
                  id = index === 5000 ? n : index > 5000 ? index - 1 : index;
                if (operation === "remove")
                  id = index >= 5000 ? index + 1 : index;
                if (operation === "swap")
                  id = index === 1 ? n - 2 : index === n - 2 ? 1 : index;
                const row = rows[index];
                if (row.dataset.id !== String(id))
                  throw Error("key order mismatch");
                let value = id;
                if (operation === "single" && id === 5000) value = 5001;
                if (
                  (operation === "bulk" && id < count) ||
                  operation === "events"
                )
                  value++;
                if (mode === "computed") value *= 2;
                if (operation === "fanout") value += 11;
                if (row.querySelector(".value").textContent !== String(value))
                  throw Error(`row ${id}: wrong text`);
                if (structural && id < n && row !== before[id])
                  throw Error(`lost keyed identity ${id}`);
              }
              const added = records
                .flatMap((record) => [...record.addedNodes])
                .filter((node) => node.nodeType === 1);
              const removed = records
                .flatMap((record) => [...record.removedNodes])
                .filter((node) => node.nodeType === 1);
              const mutations = structural
                ? {
                    inserted: added.filter((node) => !before.includes(node))
                      .length,
                    removed: removed.filter((node) => !rows.includes(node))
                      .length,
                    moves: added.filter((node) => before.includes(node)).length,
                  }
                : undefined;
              if (
                structural &&
                (mutations.inserted !== (operation === "insert" ? 1 : 0) ||
                  mutations.removed !== (operation === "remove" ? 1 : 0))
              )
                throw Error("unexpected keyed insertion/removal count");
              return { ms, mutations };
            },
            { n, mode, operation, count },
          );
          if (run >= warmups) {
            values.push(result.ms);
            mutations = result.mutations;
          }
        }
        report.results.push({
          framework,
          id,
          label,
          category,
          unit: "ms",
          n,
          status: "measured",
          samples: values,
          ...stats(values),
          mutations,
        });
        console.log(`  ${id}: ${stats(values).median.toFixed(3)} ms`);
      } catch (error) {
        report.results.push({
          framework,
          id,
          label,
          category,
          unit: "ms",
          n,
          status: "failed",
          error: error.message,
        });
        report.errors.push({ framework, id, error: error.message });
        console.error(`  FAILED ${id}: ${error.message}`);
        break;
      }
    }
    if (
      !process.env.BENCH_SKIP_MEMORY &&
      !report.errors.some((error) => error.framework === framework)
    ) {
      await page.evaluate(async () => {
        await __bench.unmount();
        await __bench.flush();
      });
      const cdp = await context.newCDPSession(page);
      const baseline = await collect(cdp, page);
      const memory = { framework, baseline, live: [], cycles: [] };
      for (const n of [10000, 100000]) {
        await page.evaluate(async (n) => {
          await __bench.mount(n, "rows");
          await __bench.flush();
        }, n);
        memory.live.push({ n, ...(await collect(cdp, page)) });
        await page.evaluate(async () => {
          await __bench.unmount();
          await __bench.flush();
        });
      }
      memory.afterLarge = await collect(cdp, page);
      // Warm allocation capacity before looking for retention over repeated cycles.
      for (let cycle = 0; cycle < 10; cycle++) {
        await page.evaluate(async () => {
          await __bench.mount(10000, "rows");
          await __bench.flush();
          await __bench.unmount();
          await __bench.flush();
        });
        memory.cycles.push({ cycle: cycle + 1, ...(await collect(cdp, page)) });
      }
      report.memory.push(memory);
    }
    for (const n of [1000, 10000]) {
      const html = await readFile(
        resolve(outputDirectory, `server-html/${framework}-${n}.html`),
        "utf8",
      );
      const values = [];
      try {
        for (let run = 0; run < warmups + samples; run++) {
          const hydrationPage = await context.newPage();
          await hydrationPage.goto(`${origin}/workloads/${framework}/`);
          await hydrationPage.waitForFunction(
            () => globalThis.__benchReady !== undefined,
          );
          const ms = await hydrationPage.evaluate(
            async ({ html, n }) => {
              if (document.querySelector("#app section")) {
                await __bench.unmount();
                await __bench.flush();
              }
              document.getElementById("app").innerHTML = html;
              // Execute the server renderer's own bootstrap once per fresh document,
              // as it would execute during HTML parsing. Never synthesize private state.
              for (const inert of document.querySelectorAll("#app script")) {
                const script = document.createElement("script");
                script.textContent = inert.textContent;
                document.head.append(script);
                inert.remove();
              }
              const before = [...document.querySelectorAll("#app li")];
              const start = performance.now();
              await __bench.hydrate(n);
              await __bench.flush();
              const ms = performance.now() - start;
              const after = [...document.querySelectorAll("#app li")];
              if (
                after.length !== n ||
                after.some((node, index) => node !== before[index])
              )
                throw Error("hydration replaced server rows");
              after[0].querySelector("button").click();
              await __bench.flush();
              if (after[0].querySelector(".value").textContent !== "1")
                throw Error("hydrated handler did not run exactly once");
              return ms;
            },
            { html, n },
          );
          await hydrationPage.close();
          if (run >= warmups) values.push(ms);
        }
        report.results.push({
          framework,
          id: `hydration-${n}`,
          label: `Hydrate ${n.toLocaleString("en-US")} server rows`,
          category: "server",
          n,
          unit: "ms",
          status: "measured",
          samples: values,
          ...stats(values),
        });
        console.log(`  hydration-${n}: ${stats(values).median.toFixed(3)} ms`);
      } catch (error) {
        report.results.push({
          framework,
          id: `hydration-${n}`,
          label: `Hydrate ${n} server rows`,
          category: "server",
          n,
          unit: "ms",
          status: "failed",
          error: error.message,
        });
        report.errors.push({
          framework,
          id: `hydration-${n}`,
          error: error.message,
        });
        break;
      }
    }
    if (pageErrors.length)
      report.errors.push({
        framework,
        id: "runtime",
        error: pageErrors.join("\n"),
      });
    await context.close();
    const startup = [];
    for (let run = 0; run < samples; run++) {
      const fresh = await browser.newContext();
      const tab = await fresh.newPage();
      await tab.goto(`${origin}/workloads/${framework}/?startup=1000`);
      await tab.waitForFunction(() => globalThis.__benchReady !== undefined);
      startup.push(await tab.evaluate(() => __benchReady));
      await tab.evaluate(async () => {
        const rows = document.querySelectorAll("#app li");
        if (rows.length !== 1000) throw Error("startup row count mismatch");
        rows[0].querySelector("button").click();
        await __bench.flush();
        if (rows[0].querySelector(".value").textContent !== "1")
          throw Error("startup is not interactive");
      });
      await fresh.close();
    }
    report.results.push({
      framework,
      id: "startup",
      label: "Navigation → 1,000 interactive rows",
      category: "startup",
      unit: "ms",
      n: 1000,
      status: "measured",
      samples: startup,
      ...stats(startup),
    });
  }
} catch (error) {
  report.errors.push({ id: "harness", error: error.message });
  throw error;
} finally {
  await browser.close();
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
  await mkdir(outputDirectory, { recursive: true });
  const path = resolve(outputDirectory, "latest.json");
  await writeFile(path + ".tmp", JSON.stringify(report, null, 2) + "\n");
  await rename(path + ".tmp", path);
  const csv =
    [
      "framework,id,status,unit,median,p95,min,max",
      ...report.results.map((row) =>
        [
          row.framework,
          row.id,
          row.status,
          row.unit,
          row.median ?? "",
          row.p95 ?? "",
          row.min ?? "",
          row.max ?? "",
        ].join(","),
      ),
    ].join("\n") + "\n";
  await writeFile(resolve(outputDirectory, "latest.csv"), csv);
}
if (report.errors.length) process.exitCode = 1;
