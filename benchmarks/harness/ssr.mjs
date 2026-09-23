import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { execFileSync } from "node:child_process";
import { root } from "./server.mjs";
process.env.NODE_ENV = "production";
const outputDirectory = resolve(
  root,
  process.env.BENCH_OUTPUT_DIR || "target/benchmarks/current",
);
const frameworks = ["fusor", "react", "svelte", "solid", "vue", "preact"];
const records = [];
const build = JSON.parse(
  await readFile(resolve(root, "benchmarks/dist/build.json"), "utf8"),
);
for (const framework of frameworks) {
  const render =
    framework === "fusor"
      ? null
      : (
          await import(
            pathToFileURL(
              resolve(root, "benchmarks/dist/server", framework, "render.js"),
            ).href
          )
        ).render;
  for (const n of [1000, 10000]) {
    let html, samples;
    if (render) {
      for (let i = 0; i < 3; i++) await render(n);
      samples = [];
      for (let i = 0; i < 15; i++) {
        const start = performance.now();
        html = await render(n);
        samples.push(performance.now() - start);
      }
    } else {
      const binary = build.nativeSsr;
      const result = JSON.parse(
        execFileSync(binary, [String(n)], {
          encoding: "utf8",
          maxBuffer: 50 * 1024 * 1024,
        }),
      );
      samples = result.samples;
      html = execFileSync(binary, [String(n), "html"], {
        encoding: "utf8",
        maxBuffer: 50 * 1024 * 1024,
      });
    }
    if ((html.match(/<li\b/g) || []).length !== n)
      throw Error(`${framework} SSR row count mismatch`);
    const sorted = [...samples].sort((a, b) => a - b);
    records.push({
      framework,
      id: `ssr-${n}`,
      label: `Server render · ${n.toLocaleString("en-US")}`,
      category: "server",
      n,
      unit: "ms",
      status: "measured",
      samples,
      median: sorted[7],
      p95: sorted[14],
      min: sorted[0],
      max: sorted[14],
      htmlBytes: Buffer.byteLength(html),
    });
    // Run output, not a build artifact: the receipt fingerprints benchmarks/dist.
    await mkdir(resolve(outputDirectory, "server-html"), { recursive: true });
    await writeFile(
      resolve(outputDirectory, `server-html/${framework}-${n}.html`),
      html,
    );
    console.log(
      `SSR ${framework} ${n}: ${sorted[7].toFixed(3)} ms · ${Buffer.byteLength(html)} HTML bytes`,
    );
  }
}
await mkdir(outputDirectory, { recursive: true });
await writeFile(
  resolve(outputDirectory, "ssr.json"),
  JSON.stringify(
    { schema: 1, generatedAt: new Date().toISOString(), records },
    null,
    2,
  ) + "\n",
);
